use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Duration as ChronoDuration;
use cilica_agent::{Agent, FakeNotes, StaticProviders};
use cilica_domain::{AppEvent, ErrorCode, ProposalKind, RecordingSink, TaskRunStatus, ToastAction, ToastKind, now};
use cilica_llm::mock::{MockProvider, MockResponse};
use serde_json::json;

use super::*;

struct Env {
    scheduler: Scheduler,
    db: Db,
    notes: Arc<FakeNotes>,
    mock: Arc<MockProvider>,
    sink: Arc<RecordingSink>,
    recording: Arc<AtomicBool>,
}

async fn env() -> Env {
    let db = Db::open_in_memory().await.unwrap();
    let notes = Arc::new(FakeNotes::new());
    let mock = Arc::new(MockProvider::new(vec![]).with_fallback("Nothing to change."));
    let sink = Arc::new(RecordingSink::default());
    let agent = Agent::new(db.clone(), notes.clone(), sink.clone(), Arc::new(StaticProviders::new(mock.clone())));
    let recording = Arc::new(AtomicBool::new(false));
    let flag = recording.clone();
    let scheduler = Scheduler::new(db.clone(), agent, sink.clone(), Arc::new(move || flag.load(Ordering::SeqCst)));
    Env { scheduler, db, notes, mock, sink, recording }
}

impl Env {
    async fn make_due(&self) -> Task {
        let task = self.db.list_tasks().await.unwrap().into_iter().next().unwrap();
        self.db.set_task_next_run(task.id, Some(now() - ChronoDuration::days(4))).await.unwrap();
        task
    }
}

#[tokio::test]
async fn seeds_notes_review_once_with_a_full_period_wait() {
    let e = env().await;
    e.scheduler.init().await.unwrap();
    e.scheduler.init().await.unwrap();
    let tasks = e.scheduler.list_tasks().await.unwrap();
    assert_eq!(tasks.len(), 1);
    let t = &tasks[0];
    assert!(t.is_default && t.enabled);
    assert_eq!(t.name, "Notes review");
    assert_eq!(t.schedule, TaskSchedule::EveryNDays { days: 3, hour: 9, minute: 0 });
    let next = t.next_run_at.unwrap();
    assert!(next > now() + ChronoDuration::days(2) && next < now() + ChronoDuration::days(4));
    // The default cannot be deleted.
    assert_eq!(e.scheduler.delete_task(t.id).await.unwrap_err().code, ErrorCode::Invalid);
}

#[tokio::test]
async fn missed_run_executes_once_then_moves_forward() {
    let e = env().await;
    e.scheduler.init().await.unwrap();
    let empty = e.notes.add_note("Untitled", "");
    let task = e.make_due().await;
    e.mock.push(MockResponse::tool("trash_note", json!({"note_ids": [empty.id.to_string()], "reason": "It is empty."})));
    e.mock.push(MockResponse::text("Suggested moving 1 empty note to Trash."));

    let run = e.scheduler.tick().await.unwrap().expect("ran");
    assert_eq!(run.status, TaskRunStatus::Succeeded);
    assert_eq!(run.summary, "Suggested moving 1 empty note to Trash.");
    let detail = e.scheduler.get_task_run(run.id).await.unwrap();
    assert_eq!(detail.proposals.len(), 1);
    assert!(matches!(detail.proposals[0].kind, ProposalKind::TrashNotes { .. }));
    assert_eq!(e.notes.live_count(), 1, "only proposed");

    let task = e.db.get_task(task.id).await.unwrap();
    assert!(task.next_run_at.unwrap() > now());
    assert_eq!(task.pending_count, 1);
    // Not due any more: one run for any number of missed periods.
    assert!(e.scheduler.tick().await.unwrap().is_none());

    // Finished event and a toast that opens the report.
    let events = e.sink.0.lock().unwrap().clone();
    assert!(events.iter().any(|ev| matches!(ev, AppEvent::TaskRunFinished { run: r } if r.id == run.id)));
    assert!(events.iter().any(|ev| matches!(ev, AppEvent::Toast { toast } if toast.kind == ToastKind::Action
        && toast.title == "Notes review found 1 suggestion"
        && toast.action == Some(ToastAction::OpenTaskRun { task_run_id: run.id }))));
}

#[tokio::test]
async fn never_runs_while_recording() {
    let e = env().await;
    e.scheduler.init().await.unwrap();
    let task = e.make_due().await;
    e.recording.store(true, Ordering::SeqCst);
    assert!(e.scheduler.tick().await.unwrap().is_none());
    assert_eq!(e.scheduler.run_now(task.id).await.unwrap_err().code, ErrorCode::Conflict);
    assert!(e.mock.requests().is_empty());
    e.recording.store(false, Ordering::SeqCst);
    assert!(e.scheduler.tick().await.unwrap().is_some());
}

#[tokio::test]
async fn a_recording_that_starts_cancels_the_run() {
    let e = env().await;
    e.scheduler.init().await.unwrap();
    let task = e.make_due().await;
    e.mock.push(MockResponse::Hang { text: "Reviewing".into() });
    let flag = e.recording.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        flag.store(true, Ordering::SeqCst);
    });
    let run = e.scheduler.tick().await.unwrap().unwrap();
    assert_eq!(run.status, TaskRunStatus::Cancelled);
    assert_eq!(run.summary, "Stopped because a recording started.");
    assert!(e.db.get_task(task.id).await.unwrap().next_run_at.unwrap() > now());
}

#[tokio::test]
async fn one_run_at_a_time_and_run_now_is_background() {
    let e = env().await;
    e.scheduler.init().await.unwrap();
    let task = e.db.list_tasks().await.unwrap().remove(0);
    e.mock.push(MockResponse::Hang { text: "Reviewing".into() });
    let run = e.scheduler.run_now(task.id).await.unwrap();
    assert_eq!(run.status, TaskRunStatus::Running);
    let busy = e.scheduler.run_now(task.id).await.unwrap_err();
    assert_eq!(busy.code, ErrorCode::Conflict);
    e.scheduler.cancel_current();
    for _ in 0..200 {
        if e.db.get_task_run(run.id).await.unwrap().status != TaskRunStatus::Running {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(e.db.get_task_run(run.id).await.unwrap().status, TaskRunStatus::Cancelled);
    // Free again.
    let again = e.scheduler.run_now(task.id).await.unwrap();
    assert_eq!(again.status, TaskRunStatus::Running);
}

#[tokio::test]
async fn crud_validates_and_schedules() {
    let e = env().await;
    let draft = TaskDraft {
        name: "Friday digest".into(),
        prompt: "Write a digest of the week.".into(),
        scope: TaskScope::LastDays { days: 7 },
        schedule: TaskSchedule::Weekly { weekday: 4, hour: 16, minute: 0 },
        enabled: true,
    };
    let t = e.scheduler.create_task(draft.clone()).await.unwrap();
    assert!(t.next_run_at.unwrap() > now());
    let mut off = draft.clone();
    off.enabled = false;
    assert!(e.scheduler.update_task(t.id, off).await.unwrap().next_run_at.is_none());
    let mut bad = draft.clone();
    bad.schedule = TaskSchedule::Daily { hour: 24, minute: 0 };
    assert_eq!(e.scheduler.create_task(bad).await.unwrap_err().code, ErrorCode::Invalid);
    e.scheduler.delete_task(t.id).await.unwrap();
    assert!(e.scheduler.list_tasks().await.unwrap().is_empty());
}

#[tokio::test]
async fn failures_are_reported_with_an_error_toast() {
    let e = env().await;
    e.scheduler.init().await.unwrap();
    e.make_due().await;
    e.mock.push(MockResponse::Error(cilica_llm::LlmError::Overloaded { provider: "Anthropic".into() }));
    let run = e.scheduler.tick().await.unwrap().unwrap();
    assert_eq!(run.status, TaskRunStatus::Failed);
    assert_eq!(run.error.as_deref(), Some("Anthropic is overloaded right now. Try again in a minute."));
    let events = e.sink.0.lock().unwrap().clone();
    assert!(events.iter().any(|ev| matches!(ev, AppEvent::Toast { toast } if toast.kind == ToastKind::Error)));
}
