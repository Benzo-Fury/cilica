//! threads, messages, proposals, ai_actions. Owner: AI agent.
//!
//! Ids and timestamps use the store's `codec` (TEXT ids, fixed-width RFC 3339), enums are stored as
//! their snake_case serde names. Display parts and raw provider content are stored per message.

use cilica_domain::{
    AiAction, AiActionStatus, AiSettings, Id, Message, MessagePart, Proposal, ProposalKind, ProposalSource, ProposalStatus, Role, Settings, Thread,
    Timestamp, ToolTier,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::{Db, StoreError, StoreResult};

// ── Helpers shared with tasks.rs ─────────────────────────────────────────

pub(crate) use crate::codec::{parse_id, parse_opt_id, parse_opt_ts, parse_ts};

/// Same fixed-width format as the rest of the store (see `codec::ts`).
pub(crate) fn ts(t: Timestamp) -> String {
    crate::codec::ts(t)
}

/// The serde name of a unit enum variant ("pending", "destructive").
pub(crate) fn enum_str<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(other) => other.to_string(),
        Err(_) => String::new(),
    }
}

pub(crate) fn enum_from<T: DeserializeOwned>(s: &str) -> StoreResult<T> {
    Ok(serde_json::from_value(serde_json::Value::String(s.to_string()))?)
}

// ── Rows ─────────────────────────────────────────────────────────────────

fn thread_from(row: &SqliteRow) -> StoreResult<Thread> {
    Ok(Thread {
        id: parse_id(row.try_get("id")?)?,
        title: row.try_get("title")?,
        created_at: parse_ts(row.try_get("created_at")?)?,
        updated_at: parse_ts(row.try_get("updated_at")?)?,
    })
}

fn message_from(row: &SqliteRow) -> StoreResult<(Message, Option<String>)> {
    let parts: Vec<MessagePart> = serde_json::from_str(row.try_get("parts_json")?)?;
    Ok((
        Message {
            id: parse_id(row.try_get("id")?)?,
            thread_id: parse_id(row.try_get("thread_id")?)?,
            turn_id: parse_id(row.try_get("turn_id")?)?,
            role: enum_from::<Role>(row.try_get("role")?)?,
            parts,
            created_at: parse_ts(row.try_get("created_at")?)?,
        },
        row.try_get("raw_json")?,
    ))
}

fn proposal_from(row: &SqliteRow) -> StoreResult<Proposal> {
    let kind: ProposalKind = serde_json::from_str(row.try_get("payload_json")?)?;
    Ok(Proposal {
        id: parse_id(row.try_get("id")?)?,
        source: enum_from::<ProposalSource>(row.try_get("source")?)?,
        thread_id: parse_opt_id(row.try_get("thread_id")?)?,
        task_run_id: parse_opt_id(row.try_get("task_run_id")?)?,
        tier: enum_from::<ToolTier>(row.try_get("tier")?)?,
        kind,
        reason: row.try_get("reason")?,
        status: enum_from::<ProposalStatus>(row.try_get("status")?)?,
        created_at: parse_ts(row.try_get("created_at")?)?,
        resolved_at: parse_opt_ts(row.try_get("resolved_at")?)?,
    })
}

fn action_from(row: &SqliteRow) -> StoreResult<(AiAction, Option<String>)> {
    let status = enum_from::<AiActionStatus>(row.try_get("status")?)?;
    let undo_json: Option<String> = row.try_get("undo_json")?;
    Ok((
        AiAction {
            id: parse_id(row.try_get("id")?)?,
            thread_id: parse_opt_id(row.try_get("thread_id")?)?,
            turn_id: parse_id(row.try_get("turn_id")?)?,
            tool: row.try_get("tool")?,
            tier: enum_from::<ToolTier>(row.try_get("tier")?)?,
            args_json: row.try_get("args_json")?,
            summary: row.try_get("summary")?,
            status,
            error: row.try_get("error")?,
            can_undo: status == AiActionStatus::Applied && undo_json.is_some(),
            proposal_id: parse_opt_id(row.try_get("proposal_id")?)?,
            created_at: parse_ts(row.try_get("created_at")?)?,
        },
        undo_json,
    ))
}

/// The name a `ProposalKind` is stored under in the `kind` column.
fn kind_name(kind: &ProposalKind) -> &'static str {
    match kind {
        ProposalKind::EditNote { .. } => "edit_note",
        ProposalKind::RenameNote { .. } => "rename_note",
        ProposalKind::TrashNotes { .. } => "trash_notes",
        ProposalKind::MergeNotes { .. } => "merge_notes",
        ProposalKind::DeleteCategory { .. } => "delete_category",
        ProposalKind::DeletePerson { .. } => "delete_person",
        ProposalKind::BulkMove { .. } => "bulk_move",
        ProposalKind::CreateNote { .. } => "create_note",
        ProposalKind::MoveGroups { .. } => "move_groups",
    }
}

impl Db {
    // ── Threads ──────────────────────────────────────────────────────────

    pub async fn create_thread(&self, title: &str) -> StoreResult<Thread> {
        let now = cilica_domain::now();
        let thread = Thread { id: cilica_domain::new_id(), title: title.to_string(), created_at: now, updated_at: now };
        sqlx::query("INSERT INTO threads (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(thread.id.to_string())
            .bind(&thread.title)
            .bind(ts(now))
            .bind(ts(now))
            .execute(self.pool())
            .await?;
        Ok(thread)
    }

    /// Newest activity first.
    pub async fn list_threads(&self) -> StoreResult<Vec<Thread>> {
        let rows = sqlx::query("SELECT * FROM threads ORDER BY updated_at DESC, rowid DESC").fetch_all(self.pool()).await?;
        rows.iter().map(thread_from).collect()
    }

    pub async fn get_thread(&self, id: Id) -> StoreResult<Thread> {
        let row = sqlx::query("SELECT * FROM threads WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(|| StoreError::NotFound("That conversation".into()))?;
        thread_from(&row)
    }

    pub async fn rename_thread(&self, id: Id, title: &str) -> StoreResult<Thread> {
        sqlx::query("UPDATE threads SET title = ? WHERE id = ?").bind(title).bind(id.to_string()).execute(self.pool()).await?;
        self.get_thread(id).await
    }

    /// Bump `updated_at` (a new message arrived).
    pub async fn touch_thread(&self, id: Id) -> StoreResult<()> {
        sqlx::query("UPDATE threads SET updated_at = ? WHERE id = ?")
            .bind(ts(cilica_domain::now()))
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Deletes the thread and its messages. Proposals and audit rows stay (their thread link is cleared).
    pub async fn delete_thread(&self, id: Id) -> StoreResult<()> {
        let done = sqlx::query("DELETE FROM threads WHERE id = ?").bind(id.to_string()).execute(self.pool()).await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound("That conversation".into()));
        }
        Ok(())
    }

    /// Delete every thread with no activity since `cutoff`, except those in `keep` (threads with a
    /// running turn). Returns the ids deleted. Proposals and audit rows stay, as with [`Db::delete_thread`].
    pub async fn delete_threads_idle_since(&self, cutoff: Timestamp, keep: &[Id]) -> StoreResult<Vec<Id>> {
        let rows = sqlx::query("SELECT id FROM threads WHERE updated_at < ?").bind(ts(cutoff)).fetch_all(self.pool()).await?;
        let mut deleted = Vec::new();
        for row in &rows {
            let id = parse_id(row.try_get("id")?)?;
            if keep.contains(&id) {
                continue;
            }
            sqlx::query("DELETE FROM threads WHERE id = ?").bind(id.to_string()).execute(self.pool()).await?;
            deleted.push(id);
        }
        Ok(deleted)
    }

    /// Provider-side session id (Claude Code `--resume`).
    pub async fn thread_external_id(&self, id: Id) -> StoreResult<Option<String>> {
        let row = sqlx::query("SELECT external_id FROM threads WHERE id = ?").bind(id.to_string()).fetch_optional(self.pool()).await?;
        Ok(row.and_then(|r| r.try_get::<Option<String>, _>("external_id").ok().flatten()))
    }

    pub async fn set_thread_external_id(&self, id: Id, external_id: Option<&str>) -> StoreResult<()> {
        sqlx::query("UPDATE threads SET external_id = ? WHERE id = ?").bind(external_id).bind(id.to_string()).execute(self.pool()).await?;
        Ok(())
    }

    // ── Messages ─────────────────────────────────────────────────────────

    /// Insert or replace a message (assistant messages are saved as they grow).
    pub async fn save_message(&self, message: &Message, raw_json: Option<&str>) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO messages (id, thread_id, turn_id, role, parts_json, raw_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET parts_json = excluded.parts_json, raw_json = excluded.raw_json",
        )
        .bind(message.id.to_string())
        .bind(message.thread_id.to_string())
        .bind(message.turn_id.to_string())
        .bind(enum_str(&message.role))
        .bind(serde_json::to_string(&message.parts)?)
        .bind(raw_json)
        .bind(ts(message.created_at))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Oldest first, with the raw provider content of each.
    pub async fn list_messages(&self, thread_id: Id) -> StoreResult<Vec<(Message, Option<String>)>> {
        let rows = sqlx::query("SELECT * FROM messages WHERE thread_id = ? ORDER BY created_at, rowid")
            .bind(thread_id.to_string())
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(message_from).collect()
    }

    // ── Proposals ────────────────────────────────────────────────────────

    pub async fn insert_proposal(&self, p: &Proposal, fingerprint: Option<&str>) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO proposals (id, source, thread_id, task_run_id, tier, kind, payload_json, reason, status, fingerprint, created_at, resolved_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(p.id.to_string())
        .bind(enum_str(&p.source))
        .bind(p.thread_id.map(|i| i.to_string()))
        .bind(p.task_run_id.map(|i| i.to_string()))
        .bind(enum_str(&p.tier))
        .bind(kind_name(&p.kind))
        .bind(serde_json::to_string(&p.kind)?)
        .bind(&p.reason)
        .bind(enum_str(&p.status))
        .bind(fingerprint)
        .bind(ts(p.created_at))
        .bind(p.resolved_at.map(ts))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Replace what a still-pending proposal would do. Returns false when it was already resolved,
    /// so a card the user has answered is never changed under them.
    pub async fn update_pending_proposal_kind(&self, id: Id, kind: &ProposalKind) -> StoreResult<bool> {
        let done = sqlx::query("UPDATE proposals SET payload_json = ?, kind = ? WHERE id = ? AND status = 'pending'")
            .bind(serde_json::to_string(kind)?)
            .bind(kind_name(kind))
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn get_proposal(&self, id: Id) -> StoreResult<Proposal> {
        let row = sqlx::query("SELECT * FROM proposals WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(|| StoreError::NotFound("That suggestion".into()))?;
        proposal_from(&row)
    }

    /// Newest first, optionally filtered by status.
    pub async fn list_proposals(&self, status: Option<ProposalStatus>) -> StoreResult<Vec<Proposal>> {
        let rows = match status {
            Some(s) => {
                sqlx::query("SELECT * FROM proposals WHERE status = ? ORDER BY created_at DESC, rowid DESC")
                    .bind(enum_str(&s))
                    .fetch_all(self.pool())
                    .await?
            }
            None => sqlx::query("SELECT * FROM proposals ORDER BY created_at DESC, rowid DESC").fetch_all(self.pool()).await?,
        };
        rows.iter().map(proposal_from).collect()
    }

    /// A task run's proposals, in the order they were made.
    pub async fn list_proposals_for_run(&self, task_run_id: Id) -> StoreResult<Vec<Proposal>> {
        let rows = sqlx::query("SELECT * FROM proposals WHERE task_run_id = ? ORDER BY created_at, rowid")
            .bind(task_run_id.to_string())
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(proposal_from).collect()
    }

    /// Atomically move a proposal from `from` to `to`. Returns false if it was not in `from`
    /// (someone else applied or dismissed it first).
    pub async fn transition_proposal(&self, id: Id, from: ProposalStatus, to: ProposalStatus) -> StoreResult<bool> {
        let resolved = (to != ProposalStatus::Pending).then(|| ts(cilica_domain::now()));
        let done = sqlx::query("UPDATE proposals SET status = ?, resolved_at = ? WHERE id = ? AND status = ?")
            .bind(enum_str(&to))
            .bind(resolved)
            .bind(id.to_string())
            .bind(enum_str(&from))
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() == 1)
    }

    /// The status of an earlier proposal with this fingerprint: dismissed wins, then pending, then applied.
    pub async fn proposal_fingerprint_status(&self, fingerprint: &str) -> StoreResult<Option<ProposalStatus>> {
        let rows = sqlx::query("SELECT status FROM proposals WHERE fingerprint = ?").bind(fingerprint).fetch_all(self.pool()).await?;
        let statuses: Vec<ProposalStatus> =
            rows.iter().map(|r| enum_from::<ProposalStatus>(r.try_get::<&str, _>("status")?)).collect::<StoreResult<_>>()?;
        Ok([ProposalStatus::Dismissed, ProposalStatus::Pending, ProposalStatus::Applied].into_iter().find(|s| statuses.contains(s)))
    }

    // ── Audit log ────────────────────────────────────────────────────────

    pub async fn insert_ai_action(&self, a: &AiAction, undo_json: Option<&str>) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO ai_actions (id, thread_id, turn_id, tool, tier, args_json, summary, status, error, undo_json, proposal_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(a.id.to_string())
        .bind(a.thread_id.map(|i| i.to_string()))
        .bind(a.turn_id.to_string())
        .bind(&a.tool)
        .bind(enum_str(&a.tier))
        .bind(&a.args_json)
        .bind(&a.summary)
        .bind(enum_str(&a.status))
        .bind(&a.error)
        .bind(undo_json)
        .bind(a.proposal_id.map(|i| i.to_string()))
        .bind(ts(a.created_at))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Overwrite the mutable fields (summary, status, error, undo data, proposal link, tier).
    pub async fn update_ai_action(&self, a: &AiAction, undo_json: Option<&str>) -> StoreResult<()> {
        sqlx::query("UPDATE ai_actions SET tier = ?, summary = ?, status = ?, error = ?, undo_json = ?, proposal_id = ? WHERE id = ?")
            .bind(enum_str(&a.tier))
            .bind(&a.summary)
            .bind(enum_str(&a.status))
            .bind(&a.error)
            .bind(undo_json)
            .bind(a.proposal_id.map(|i| i.to_string()))
            .bind(a.id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// The row and its undo data.
    pub async fn get_ai_action(&self, id: Id) -> StoreResult<(AiAction, Option<String>)> {
        let row = sqlx::query("SELECT * FROM ai_actions WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(|| StoreError::NotFound("That action".into()))?;
        action_from(&row)
    }

    /// Newest first.
    pub async fn list_ai_actions(&self, limit: u32, offset: u32) -> StoreResult<Vec<AiAction>> {
        let rows = sqlx::query("SELECT * FROM ai_actions ORDER BY created_at DESC, rowid DESC LIMIT ? OFFSET ?")
            .bind(i64::from(limit))
            .bind(i64::from(offset))
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(|r| action_from(r).map(|(a, _)| a)).collect()
    }

    /// A turn's actions in the order they ran, with undo data.
    pub async fn list_ai_actions_for_turn(&self, turn_id: Id) -> StoreResult<Vec<(AiAction, Option<String>)>> {
        let rows = sqlx::query("SELECT * FROM ai_actions WHERE turn_id = ? ORDER BY created_at, rowid")
            .bind(turn_id.to_string())
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(action_from).collect()
    }

    pub async fn ai_action_for_proposal(&self, proposal_id: Id) -> StoreResult<Option<(AiAction, Option<String>)>> {
        let row = sqlx::query("SELECT * FROM ai_actions WHERE proposal_id = ? ORDER BY created_at DESC LIMIT 1")
            .bind(proposal_id.to_string())
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(action_from).transpose()
    }

    /// Every audit row that points at a proposal, oldest first. A grouped card collects several.
    pub async fn ai_actions_for_proposal(&self, proposal_id: Id) -> StoreResult<Vec<(AiAction, Option<String>)>> {
        let rows = sqlx::query("SELECT * FROM ai_actions WHERE proposal_id = ? ORDER BY created_at, rowid")
            .bind(proposal_id.to_string())
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(action_from).collect()
    }

    // ── Settings (read-only view for the assistant) ─────────────────────

    /// The AI settings from the settings document, defaults when unset or unreadable.
    pub async fn ai_settings(&self) -> StoreResult<AiSettings> {
        let row = sqlx::query("SELECT value_json FROM settings WHERE key = 'settings'").fetch_optional(self.pool()).await?;
        let Some(row) = row else {
            return Ok(AiSettings::default());
        };
        let json: String = row.try_get("value_json")?;
        Ok(serde_json::from_str::<Settings>(&json).map(|s| s.ai).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use cilica_domain::{NoteRef, new_id};

    /// `now()` at the store's precision, so round-trips compare equal.
    fn now() -> Timestamp {
        crate::codec::parse_ts(&ts(cilica_domain::now())).unwrap()
    }

    use super::*;

    fn proposal(fp_note: Id) -> Proposal {
        Proposal {
            id: new_id(),
            source: ProposalSource::Chat,
            thread_id: None,
            task_run_id: None,
            tier: ToolTier::Destructive,
            kind: ProposalKind::TrashNotes { notes: vec![NoteRef { id: fp_note, title: "Old".into() }] },
            reason: "Empty".into(),
            status: ProposalStatus::Pending,
            created_at: now(),
            resolved_at: None,
        }
    }

    fn action(turn: Id, thread: Option<Id>) -> AiAction {
        AiAction {
            id: new_id(),
            thread_id: thread,
            turn_id: turn,
            tool: "create_note".into(),
            tier: ToolTier::Additive,
            args_json: "{}".into(),
            summary: "Created Plan".into(),
            status: AiActionStatus::Running,
            error: None,
            can_undo: false,
            proposal_id: None,
            created_at: now(),
        }
    }

    #[tokio::test]
    async fn threads_and_messages_round_trip() {
        let db = Db::open_in_memory().await.unwrap();
        let t = db.create_thread("").await.unwrap();
        let turn = new_id();
        let mut m = Message {
            id: new_id(),
            thread_id: t.id,
            turn_id: turn,
            role: Role::User,
            parts: vec![MessagePart::Text { text: "hi".into() }],
            created_at: now(),
        };
        db.save_message(&m, Some("[]")).await.unwrap();
        m.parts.push(MessagePart::Error { message: "x".into() });
        db.save_message(&m, Some("[1]")).await.unwrap();
        let msgs = db.list_messages(t.id).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].0, m);
        assert_eq!(msgs[0].1.as_deref(), Some("[1]"));
        db.rename_thread(t.id, "Acme prep").await.unwrap();
        db.set_thread_external_id(t.id, Some("sess")).await.unwrap();
        assert_eq!(db.thread_external_id(t.id).await.unwrap().as_deref(), Some("sess"));
        assert_eq!(db.list_threads().await.unwrap()[0].title, "Acme prep");
        db.delete_thread(t.id).await.unwrap();
        assert!(db.list_messages(t.id).await.unwrap().is_empty());
        assert!(db.get_thread(t.id).await.is_err());
    }

    #[tokio::test]
    async fn idle_threads_are_deleted_except_busy_ones() {
        let db = Db::open_in_memory().await.unwrap();
        let old = db.create_thread("old").await.unwrap();
        let busy = db.create_thread("busy").await.unwrap();
        let cutoff = now() + chrono::Duration::seconds(1);
        let fresh_after_cutoff = cutoff + chrono::Duration::seconds(1);
        let fresh = db.create_thread("fresh").await.unwrap();
        sqlx::query("UPDATE threads SET updated_at = ? WHERE id = ?")
            .bind(ts(fresh_after_cutoff))
            .bind(fresh.id.to_string())
            .execute(db.pool())
            .await
            .unwrap();
        let deleted = db.delete_threads_idle_since(cutoff, &[busy.id]).await.unwrap();
        assert_eq!(deleted, vec![old.id]);
        let left: Vec<Id> = db.list_threads().await.unwrap().iter().map(|t| t.id).collect();
        assert!(left.contains(&busy.id) && left.contains(&fresh.id) && !left.contains(&old.id));
    }

    #[tokio::test]
    async fn proposals_transition_once_and_remember_fingerprints() {
        let db = Db::open_in_memory().await.unwrap();
        let p = proposal(new_id());
        db.insert_proposal(&p, Some("fp1")).await.unwrap();
        assert_eq!(db.get_proposal(p.id).await.unwrap(), p);
        assert_eq!(db.proposal_fingerprint_status("fp1").await.unwrap(), Some(ProposalStatus::Pending));
        assert!(db.transition_proposal(p.id, ProposalStatus::Pending, ProposalStatus::Dismissed).await.unwrap());
        assert!(!db.transition_proposal(p.id, ProposalStatus::Pending, ProposalStatus::Applied).await.unwrap());
        let got = db.get_proposal(p.id).await.unwrap();
        assert_eq!(got.status, ProposalStatus::Dismissed);
        assert!(got.resolved_at.is_some());
        assert_eq!(db.proposal_fingerprint_status("fp1").await.unwrap(), Some(ProposalStatus::Dismissed));
        assert_eq!(db.proposal_fingerprint_status("nope").await.unwrap(), None);
        assert_eq!(db.list_proposals(Some(ProposalStatus::Pending)).await.unwrap().len(), 0);
        assert_eq!(db.list_proposals(None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn audit_rows_update_and_list() {
        let db = Db::open_in_memory().await.unwrap();
        let t = db.create_thread("x").await.unwrap();
        let turn = new_id();
        let mut a = action(turn, Some(t.id));
        db.insert_ai_action(&a, None).await.unwrap();
        a.status = AiActionStatus::Applied;
        db.update_ai_action(&a, Some(r#"{"op":"x"}"#)).await.unwrap();
        let (got, undo) = db.get_ai_action(a.id).await.unwrap();
        assert!(got.can_undo);
        assert_eq!(undo.as_deref(), Some(r#"{"op":"x"}"#));
        let b = action(turn, None);
        db.insert_ai_action(&b, None).await.unwrap();
        assert_eq!(db.list_ai_actions_for_turn(turn).await.unwrap().len(), 2);
        assert_eq!(db.list_ai_actions(1, 0).await.unwrap()[0].id, b.id);
        // Deleting the thread keeps the audit row.
        db.delete_thread(t.id).await.unwrap();
        assert_eq!(db.get_ai_action(a.id).await.unwrap().0.thread_id, None);
    }

    #[tokio::test]
    async fn ai_settings_default_when_missing() {
        let db = Db::open_in_memory().await.unwrap();
        assert_eq!(db.ai_settings().await.unwrap(), AiSettings::default());
        let mut s = Settings::default();
        s.ai.auto_apply_edits = true;
        sqlx::query("INSERT INTO settings (key, value_json) VALUES ('settings', ?)")
            .bind(serde_json::to_string(&s).unwrap())
            .execute(db.pool())
            .await
            .unwrap();
        assert!(db.ai_settings().await.unwrap().auto_apply_edits);
    }
}
