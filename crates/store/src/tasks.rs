//! tasks, task_runs. Owner: AI agent.

use margin_domain::{Id, Task, TaskDraft, TaskRun, TaskRunStatus, Timestamp};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::assistant::{enum_from, enum_str, parse_id, parse_opt_ts, parse_ts, ts};
use crate::{Db, StoreError, StoreResult};

fn run_from(row: &SqliteRow) -> StoreResult<TaskRun> {
    Ok(TaskRun {
        id: parse_id(row.try_get("id")?)?,
        task_id: parse_id(row.try_get("task_id")?)?,
        started_at: parse_ts(row.try_get("started_at")?)?,
        finished_at: parse_opt_ts(row.try_get("finished_at")?)?,
        status: enum_from::<TaskRunStatus>(row.try_get("status")?)?,
        summary: row.try_get("summary")?,
        error: row.try_get("error")?,
    })
}

impl Db {
    async fn task_from(&self, row: &SqliteRow) -> StoreResult<Task> {
        let id = parse_id(row.try_get("id")?)?;
        let pending: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM proposals p JOIN task_runs r ON r.id = p.task_run_id WHERE r.task_id = ? AND p.status = 'pending'",
        )
        .bind(id.to_string())
        .fetch_one(self.pool())
        .await?;
        Ok(Task {
            id,
            name: row.try_get("name")?,
            prompt: row.try_get("prompt")?,
            scope: serde_json::from_str(row.try_get("scope_json")?)?,
            schedule: serde_json::from_str(row.try_get("schedule_json")?)?,
            next_run_at: parse_opt_ts(row.try_get("next_run_at")?)?,
            enabled: row.try_get::<i64, _>("enabled")? != 0,
            is_default: row.try_get::<i64, _>("is_default")? != 0,
            last_run: self.latest_task_run(id).await?,
            pending_count: u32::try_from(pending).unwrap_or(u32::MAX),
        })
    }

    pub async fn insert_task(&self, draft: &TaskDraft, is_default: bool, next_run_at: Option<Timestamp>) -> StoreResult<Task> {
        let id = margin_domain::new_id();
        sqlx::query(
            "INSERT INTO tasks (id, name, prompt, scope_json, schedule_json, next_run_at, enabled, is_default, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(&draft.name)
        .bind(&draft.prompt)
        .bind(serde_json::to_string(&draft.scope)?)
        .bind(serde_json::to_string(&draft.schedule)?)
        .bind(next_run_at.map(ts))
        .bind(i64::from(draft.enabled))
        .bind(i64::from(is_default))
        .bind(ts(margin_domain::now()))
        .execute(self.pool())
        .await?;
        self.get_task(id).await
    }

    pub async fn update_task(&self, id: Id, draft: &TaskDraft, next_run_at: Option<Timestamp>) -> StoreResult<Task> {
        let done = sqlx::query("UPDATE tasks SET name = ?, prompt = ?, scope_json = ?, schedule_json = ?, enabled = ?, next_run_at = ? WHERE id = ?")
            .bind(&draft.name)
            .bind(&draft.prompt)
            .bind(serde_json::to_string(&draft.scope)?)
            .bind(serde_json::to_string(&draft.schedule)?)
            .bind(i64::from(draft.enabled))
            .bind(next_run_at.map(ts))
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound("That task".into()));
        }
        self.get_task(id).await
    }

    pub async fn set_task_next_run(&self, id: Id, next_run_at: Option<Timestamp>) -> StoreResult<()> {
        sqlx::query("UPDATE tasks SET next_run_at = ? WHERE id = ?").bind(next_run_at.map(ts)).bind(id.to_string()).execute(self.pool()).await?;
        Ok(())
    }

    /// Deletes the task, its runs and their proposals.
    pub async fn delete_task(&self, id: Id) -> StoreResult<()> {
        let done = sqlx::query("DELETE FROM tasks WHERE id = ?").bind(id.to_string()).execute(self.pool()).await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound("That task".into()));
        }
        Ok(())
    }

    pub async fn get_task(&self, id: Id) -> StoreResult<Task> {
        let row = sqlx::query("SELECT * FROM tasks WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(|| StoreError::NotFound("That task".into()))?;
        self.task_from(&row).await
    }

    /// The default task first, then by creation.
    pub async fn list_tasks(&self) -> StoreResult<Vec<Task>> {
        let rows = sqlx::query("SELECT * FROM tasks ORDER BY is_default DESC, created_at, rowid").fetch_all(self.pool()).await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            out.push(self.task_from(row).await?);
        }
        Ok(out)
    }

    pub async fn default_task_exists(&self) -> StoreResult<bool> {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE is_default = 1").fetch_one(self.pool()).await?;
        Ok(n > 0)
    }

    // ── Runs ─────────────────────────────────────────────────────────────

    pub async fn insert_task_run(&self, run: &TaskRun) -> StoreResult<()> {
        sqlx::query("INSERT INTO task_runs (id, task_id, started_at, finished_at, status, summary, error) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(run.id.to_string())
            .bind(run.task_id.to_string())
            .bind(ts(run.started_at))
            .bind(run.finished_at.map(ts))
            .bind(enum_str(&run.status))
            .bind(&run.summary)
            .bind(&run.error)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn finish_task_run(&self, id: Id, status: TaskRunStatus, summary: &str, error: Option<&str>) -> StoreResult<TaskRun> {
        sqlx::query("UPDATE task_runs SET status = ?, summary = ?, error = ?, finished_at = ? WHERE id = ?")
            .bind(enum_str(&status))
            .bind(summary)
            .bind(error)
            .bind(ts(margin_domain::now()))
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        self.get_task_run(id).await
    }

    pub async fn get_task_run(&self, id: Id) -> StoreResult<TaskRun> {
        let row = sqlx::query("SELECT * FROM task_runs WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(|| StoreError::NotFound("That run".into()))?;
        run_from(&row)
    }

    /// Newest first.
    pub async fn list_task_runs(&self, task_id: Id) -> StoreResult<Vec<TaskRun>> {
        let rows = sqlx::query("SELECT * FROM task_runs WHERE task_id = ? ORDER BY started_at DESC, rowid DESC")
            .bind(task_id.to_string())
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(run_from).collect()
    }

    pub async fn latest_task_run(&self, task_id: Id) -> StoreResult<Option<TaskRun>> {
        let row = sqlx::query("SELECT * FROM task_runs WHERE task_id = ? ORDER BY started_at DESC, rowid DESC LIMIT 1")
            .bind(task_id.to_string())
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(run_from).transpose()
    }

    /// Runs still marked running from a previous launch become failed. Returns how many.
    pub async fn fail_interrupted_task_runs(&self) -> StoreResult<u32> {
        let done = sqlx::query("UPDATE task_runs SET status = 'failed', error = ?, finished_at = ? WHERE status = 'running'")
            .bind("Margin quit before this run finished.")
            .bind(ts(margin_domain::now()))
            .execute(self.pool())
            .await?;
        Ok(u32::try_from(done.rows_affected()).unwrap_or(u32::MAX))
    }
}

#[cfg(test)]
mod tests {
    use margin_domain::{NoteRef, Proposal, ProposalKind, ProposalSource, ProposalStatus, TaskSchedule, TaskScope, ToolTier, new_id, now};

    use super::*;

    fn draft() -> TaskDraft {
        TaskDraft {
            name: "Notes review".into(),
            prompt: "Review".into(),
            scope: TaskScope::AllNotes,
            schedule: TaskSchedule::EveryNDays { days: 3, hour: 9, minute: 0 },
            enabled: true,
        }
    }

    #[tokio::test]
    async fn tasks_runs_and_pending_counts() {
        let db = Db::open_in_memory().await.unwrap();
        assert!(!db.default_task_exists().await.unwrap());
        let t = db.insert_task(&draft(), true, Some(now())).await.unwrap();
        assert!(t.is_default && t.enabled && t.last_run.is_none());
        assert!(db.default_task_exists().await.unwrap());

        let run = TaskRun {
            id: new_id(),
            task_id: t.id,
            started_at: now(),
            finished_at: None,
            status: TaskRunStatus::Running,
            summary: String::new(),
            error: None,
        };
        db.insert_task_run(&run).await.unwrap();
        let p = Proposal {
            id: new_id(),
            source: ProposalSource::Task,
            thread_id: None,
            task_run_id: Some(run.id),
            tier: ToolTier::Destructive,
            kind: ProposalKind::TrashNotes { notes: vec![NoteRef { id: new_id(), title: "x".into() }] },
            reason: String::new(),
            status: ProposalStatus::Pending,
            created_at: now(),
            resolved_at: None,
        };
        db.insert_proposal(&p, None).await.unwrap();
        let finished = db.finish_task_run(run.id, TaskRunStatus::Succeeded, "1 suggestion", None).await.unwrap();
        assert!(finished.finished_at.is_some());
        let t = db.get_task(t.id).await.unwrap();
        assert_eq!(t.pending_count, 1);
        assert_eq!(t.last_run.unwrap().summary, "1 suggestion");
        assert_eq!(db.list_proposals_for_run(run.id).await.unwrap().len(), 1);

        let mut d = draft();
        d.enabled = false;
        let t2 = db.update_task(t.id, &d, None).await.unwrap();
        assert!(!t2.enabled && t2.next_run_at.is_none());
        db.delete_task(t.id).await.unwrap();
        assert!(db.list_tasks().await.unwrap().is_empty());
        assert!(db.get_proposal(p.id).await.is_err(), "proposals go with their run");
    }

    #[tokio::test]
    async fn interrupted_runs_are_failed_at_launch() {
        let db = Db::open_in_memory().await.unwrap();
        let t = db.insert_task(&draft(), false, None).await.unwrap();
        let run = TaskRun {
            id: new_id(),
            task_id: t.id,
            started_at: now(),
            finished_at: None,
            status: TaskRunStatus::Running,
            summary: String::new(),
            error: None,
        };
        db.insert_task_run(&run).await.unwrap();
        assert_eq!(db.fail_interrupted_task_runs().await.unwrap(), 1);
        assert_eq!(db.get_task_run(run.id).await.unwrap().status, TaskRunStatus::Failed);
    }
}
