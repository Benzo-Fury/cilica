//! Scheduled tasks: CRUD, the default "Notes review", and the runner.
//!
//! - `next_run_at` is stored; schedules are local wall-clock and DST-safe ([`schedule`]).
//! - Missed runs execute once at the next launch (then the schedule moves forward from now).
//! - Never runs while recording: due runs wait, and a run in progress is cancelled when a
//!   recording starts (checked through the `is_recording` closure the shell supplies).
//! - One run at a time; runs are cancellable.
//! - Runs use the agent in propose-only mode: every write becomes a `Proposal` linked to the run.
//!   Finishing emits `TaskRunFinished` and a toast with `ToastAction::OpenTaskRun`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Local;
use cilica_agent::Agent;
use cilica_agent::prompt::NOTES_REVIEW_PROMPT;
use cilica_domain::{
    AppError, AppEvent, AppResult, DataScope, ErrorCode, Events, Id, Task, TaskDraft, TaskRun, TaskRunDetail,
    TaskRunStatus, TaskSchedule, TaskScope, ToastAction, ToastKind, ToastRequest, Timestamp, new_id, now,
};
use cilica_llm::CancellationToken;
use cilica_store::Db;

pub mod schedule;

/// Whether a recording is in progress. Supplied by the shell.
pub type IsRecording = Arc<dyn Fn() -> bool + Send + Sync>;

/// How often the runner looks for due tasks.
const TICK: Duration = Duration::from_secs(30);
/// How often a running task checks for a recording that started.
const WATCH: Duration = Duration::from_secs(1);

pub const DEFAULT_TASK_NAME: &str = "Notes review";

/// The built-in task, seeded on first launch.
pub fn default_task() -> TaskDraft {
    TaskDraft {
        name: DEFAULT_TASK_NAME.into(),
        prompt: NOTES_REVIEW_PROMPT.into(),
        scope: TaskScope::AllNotes,
        schedule: TaskSchedule::EveryNDays { days: 3, hour: 9, minute: 0 },
        enabled: true,
    }
}

struct Inner {
    db: Db,
    agent: Agent,
    events: Events,
    is_recording: IsRecording,
    /// Held for the whole of a run: one at a time.
    run_lock: Arc<tokio::sync::Mutex<()>>,
    current: Mutex<Option<(Id, CancellationToken)>>,
    shutdown: CancellationToken,
}

#[derive(Clone)]
pub struct Scheduler {
    inner: Arc<Inner>,
}

impl Scheduler {
    pub fn new(db: Db, agent: Agent, events: Events, is_recording: IsRecording) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                agent,
                events,
                is_recording,
                run_lock: Arc::new(tokio::sync::Mutex::new(())),
                current: Mutex::new(None),
                shutdown: CancellationToken::new(),
            }),
        }
    }

    fn next_after(schedule: &TaskSchedule, after: Timestamp, last_run: Option<Timestamp>) -> Timestamp {
        schedule::next_run(schedule, after, last_run, &Local)
    }

    /// Call once at launch: fail runs interrupted by a quit, seed the default task, schedule
    /// enabled tasks that have no next run.
    pub async fn init(&self) -> AppResult<()> {
        let db = &self.inner.db;
        db.fail_interrupted_task_runs().await?;
        if !db.default_task_exists().await? {
            let draft = default_task();
            // The first review waits a full period, so a new user is not greeted by suggestions.
            let first = Self::next_after(&draft.schedule, now(), Some(now()));
            db.insert_task(&draft, true, Some(first)).await?;
        }
        for task in db.list_tasks().await? {
            if task.enabled && task.next_run_at.is_none() {
                let last = task.last_run.as_ref().map(|r| r.started_at);
                db.set_task_next_run(task.id, Some(Self::next_after(&task.schedule, now(), last))).await?;
            }
        }
        Ok(())
    }

    /// Start the background loop (first tick right away, so missed runs happen at launch).
    pub fn start(&self) {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = this.tick().await {
                    tracing::warn!(error = %e.message, "scheduled task tick failed");
                }
                tokio::select! {
                    _ = tokio::time::sleep(TICK) => {}
                    _ = this.inner.shutdown.cancelled() => break,
                }
            }
        });
    }

    /// Stop the loop and cancel any run (app quit).
    pub fn shutdown(&self) {
        self.inner.shutdown.cancel();
        self.cancel_current();
    }

    /// Cancel the run in progress, if any.
    pub fn cancel_current(&self) {
        if let Some((_, token)) = self.inner.current.lock().unwrap().as_ref() {
            token.cancel();
        }
    }

    /// Run the earliest due task, if any, and wait for it. Returns the finished run.
    pub async fn tick(&self) -> AppResult<Option<TaskRun>> {
        if (self.inner.is_recording)() {
            return Ok(None);
        }
        let db = &self.inner.db;
        let current = now();
        let mut due: Vec<Task> = Vec::new();
        for task in db.list_tasks().await? {
            match task.next_run_at {
                Some(at) if task.enabled && at <= current => due.push(task),
                None if task.enabled => {
                    let last = task.last_run.as_ref().map(|r| r.started_at);
                    db.set_task_next_run(task.id, Some(Self::next_after(&task.schedule, current, last))).await?;
                }
                _ => {}
            }
        }
        due.sort_by_key(|t| t.next_run_at);
        let Some(task) = due.into_iter().next() else { return Ok(None) };
        let Ok(guard) = self.inner.run_lock.clone().try_lock_owned() else { return Ok(None) };
        let run = self.start_run(&task).await?;
        Ok(Some(self.execute(task, run, guard).await))
    }

    /// Start a run now, in the background. `TaskRunFinished` arrives when it is done.
    pub async fn run_now(&self, task_id: Id) -> AppResult<TaskRun> {
        if (self.inner.is_recording)() {
            return Err(AppError::new(ErrorCode::Conflict, "Cilica does not run tasks while recording. Try again after the meeting."));
        }
        let task = self.inner.db.get_task(task_id).await?;
        let guard = self
            .inner
            .run_lock
            .clone()
            .try_lock_owned()
            .map_err(|_| AppError::new(ErrorCode::Conflict, "Another task is running. Try again when it finishes."))?;
        let run = self.start_run(&task).await?;
        let this = self.clone();
        let started = run.clone();
        tokio::spawn(async move {
            this.execute(task, run, guard).await;
        });
        Ok(started)
    }

    async fn start_run(&self, task: &Task) -> AppResult<TaskRun> {
        let run = TaskRun {
            id: new_id(),
            task_id: task.id,
            started_at: now(),
            finished_at: None,
            status: TaskRunStatus::Running,
            summary: String::new(),
            error: None,
        };
        self.inner.db.insert_task_run(&run).await?;
        self.inner.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Tasks], note_id: None });
        Ok(run)
    }

    async fn execute(&self, task: Task, run: TaskRun, _guard: tokio::sync::OwnedMutexGuard<()>) -> TaskRun {
        let inner = &self.inner;
        let token = CancellationToken::new();
        *inner.current.lock().unwrap() = Some((run.id, token.clone()));

        // Cancel the run if a recording starts.
        let watch_token = token.clone();
        let is_recording = inner.is_recording.clone();
        let watcher = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(WATCH) => {}
                    _ = watch_token.cancelled() => return false,
                }
                if is_recording() {
                    watch_token.cancel();
                    return true;
                }
            }
        });

        let outcome = inner.agent.run_task(&task, run.id, token.clone()).await;
        token.cancel(); // stops the watcher
        let stopped_for_recording = watcher.await.unwrap_or(false);
        *inner.current.lock().unwrap() = None;

        let (status, summary, error) = match outcome {
            Ok(o) if o.cancelled => (
                TaskRunStatus::Cancelled,
                if stopped_for_recording { "Stopped because a recording started.".to_string() } else { "Stopped.".to_string() },
                None,
            ),
            Ok(o) => match o.error {
                Some(e) => (TaskRunStatus::Failed, o.summary, Some(e)),
                None => {
                    let summary = if o.summary.is_empty() {
                        if o.proposals == 0 { "Nothing to change this time.".to_string() } else { format!("{} suggestions to review.", o.proposals) }
                    } else {
                        o.summary
                    };
                    (TaskRunStatus::Succeeded, summary, None)
                }
            },
            Err(e) => (TaskRunStatus::Failed, String::new(), Some(e.message)),
        };
        let finished = match inner.db.finish_task_run(run.id, status, &summary, error.as_deref()).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "could not finish the task run");
                TaskRun { status, summary, error, finished_at: Some(now()), ..run }
            }
        };
        let next = Self::next_after(&task.schedule, now(), Some(finished.started_at));
        if let Err(e) = inner.db.set_task_next_run(task.id, task.enabled.then_some(next)).await {
            tracing::error!(error = %e, "could not schedule the next run");
        }

        let pending = inner.db.list_proposals_for_run(run.id).await.map(|p| p.len()).unwrap_or(0);
        inner.events.emit(AppEvent::TaskRunFinished { run: finished.clone() });
        inner.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Tasks, DataScope::Proposals, DataScope::AiActions], note_id: None });
        let toast = match finished.status {
            TaskRunStatus::Succeeded => Some(ToastRequest {
                kind: ToastKind::Action,
                title: match pending {
                    0 => format!("{} found nothing to change", task.name),
                    1 => format!("{} found 1 suggestion", task.name),
                    n => format!("{} found {n} suggestions", task.name),
                },
                description: None,
                action_label: Some("Open report".into()),
                action: Some(ToastAction::OpenTaskRun { task_run_id: run.id }),
            }),
            TaskRunStatus::Failed => Some(ToastRequest {
                kind: ToastKind::Error,
                title: format!("{} could not finish", task.name),
                description: finished.error.clone(),
                action_label: Some("Open report".into()),
                action: Some(ToastAction::OpenTaskRun { task_run_id: run.id }),
            }),
            _ => None,
        };
        if let Some(toast) = toast {
            inner.events.emit(AppEvent::Toast { toast });
        }
        finished
    }

    // ── CRUD for the commands ────────────────────────────────────────────

    pub async fn list_tasks(&self) -> AppResult<Vec<Task>> {
        Ok(self.inner.db.list_tasks().await?)
    }

    fn validate(draft: &TaskDraft) -> AppResult<()> {
        if draft.name.trim().is_empty() {
            return Err(AppError::invalid("Give the task a name."));
        }
        if draft.prompt.trim().is_empty() {
            return Err(AppError::invalid("Say what the task should do."));
        }
        let (hour, minute) = match draft.schedule {
            TaskSchedule::EveryNDays { days, hour, minute } => {
                if days == 0 || days > 365 {
                    return Err(AppError::invalid("Pick a number of days between 1 and 365."));
                }
                (hour, minute)
            }
            TaskSchedule::Weekly { weekday, hour, minute } => {
                if weekday > 6 {
                    return Err(AppError::invalid("Pick a day of the week."));
                }
                (hour, minute)
            }
            TaskSchedule::Daily { hour, minute } => (hour, minute),
        };
        if hour > 23 || minute > 59 {
            return Err(AppError::invalid("Pick a time between 00:00 and 23:59."));
        }
        if let TaskScope::LastDays { days: 0 } = draft.scope {
            return Err(AppError::invalid("Pick at least 1 day."));
        }
        Ok(())
    }

    pub async fn create_task(&self, draft: TaskDraft) -> AppResult<Task> {
        Self::validate(&draft)?;
        let next = draft.enabled.then(|| Self::next_after(&draft.schedule, now(), None));
        let task = self.inner.db.insert_task(&draft, false, next).await?;
        self.inner.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Tasks], note_id: None });
        Ok(task)
    }

    pub async fn update_task(&self, task_id: Id, draft: TaskDraft) -> AppResult<Task> {
        Self::validate(&draft)?;
        let existing = self.inner.db.get_task(task_id).await?;
        let last = existing.last_run.as_ref().map(|r| r.started_at);
        let next = draft.enabled.then(|| Self::next_after(&draft.schedule, now(), last));
        let task = self.inner.db.update_task(task_id, &draft, next).await?;
        self.inner.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Tasks], note_id: None });
        Ok(task)
    }

    pub async fn delete_task(&self, task_id: Id) -> AppResult<()> {
        let task = self.inner.db.get_task(task_id).await?;
        if task.is_default {
            return Err(AppError::invalid("Notes review is built in. Turn it off instead of deleting it."));
        }
        if self.inner.current.lock().unwrap().as_ref().is_some_and(|(run_id, _)| task.last_run.as_ref().is_some_and(|r| r.id == *run_id)) {
            self.cancel_current();
        }
        self.inner.db.delete_task(task_id).await?;
        self.inner.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Tasks, DataScope::Proposals], note_id: None });
        Ok(())
    }

    pub async fn list_task_runs(&self, task_id: Id) -> AppResult<Vec<TaskRun>> {
        Ok(self.inner.db.list_task_runs(task_id).await?)
    }

    pub async fn get_task_run(&self, task_run_id: Id) -> AppResult<TaskRunDetail> {
        let run = self.inner.db.get_task_run(task_run_id).await?;
        let proposals = self.inner.db.list_proposals_for_run(task_run_id).await?;
        Ok(TaskRunDetail { run, proposals })
    }
}

#[cfg(test)]
mod tests;
