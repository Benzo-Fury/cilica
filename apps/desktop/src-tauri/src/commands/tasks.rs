//! Scheduled tasks and their reports. Owner: AI agent.
//!
//! Every command here is part of the IPC contract in `src/ipc/bindings.ts`. Keep names, arguments and
//! return types as they are unless the orchestrator agrees to a change; replace the stub bodies.

use std::sync::Arc;

use margin_domain::*;
use margin_scheduler::Scheduler;
use tauri::{AppHandle, Manager, State};

use crate::core::Core;

/// Managed state for this area.
pub struct TasksState {
    pub scheduler: Scheduler,
}

/// Called once at startup, after the database is open (and after `assistant::setup`, whose agent
/// the scheduler uses). Seeds the default Notes review and starts the runner.
pub fn setup(app: &AppHandle, core: &Core) -> AppResult<()> {
    let agent = super::assistant::agent(app).ok_or_else(|| AppError::internal("The assistant must be set up before scheduled tasks."))?;
    let handle = app.clone();
    let is_recording: margin_scheduler::IsRecording = Arc::new(move || super::recording::is_recording(&handle));
    let scheduler = Scheduler::new(core.db.clone(), agent, core.events.clone(), is_recording);
    app.manage(TasksState { scheduler: scheduler.clone() });
    tauri::async_runtime::spawn(async move {
        if let Err(e) = scheduler.init().await {
            tracing::error!(error = %e.message, "scheduled tasks could not start");
        }
        scheduler.start();
    });
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn list_tasks(state: State<'_, TasksState>) -> AppResult<Vec<Task>> {
    state.scheduler.list_tasks().await
}

#[tauri::command]
#[specta::specta]
pub async fn create_task(state: State<'_, TasksState>, draft: TaskDraft) -> AppResult<Task> {
    state.scheduler.create_task(draft).await
}

#[tauri::command]
#[specta::specta]
pub async fn update_task(state: State<'_, TasksState>, task_id: Id, draft: TaskDraft) -> AppResult<Task> {
    state.scheduler.update_task(task_id, draft).await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_task(state: State<'_, TasksState>, task_id: Id) -> AppResult<()> {
    state.scheduler.delete_task(task_id).await
}

/// Starts a run in the background; TaskRunFinished arrives when it is done.
#[tauri::command]
#[specta::specta]
pub async fn run_task_now(state: State<'_, TasksState>, task_id: Id) -> AppResult<TaskRun> {
    state.scheduler.run_now(task_id).await
}

/// Newest first.
#[tauri::command]
#[specta::specta]
pub async fn list_task_runs(state: State<'_, TasksState>, task_id: Id) -> AppResult<Vec<TaskRun>> {
    state.scheduler.list_task_runs(task_id).await
}

/// The report view: the run and everything it proposed.
#[tauri::command]
#[specta::specta]
pub async fn get_task_run(state: State<'_, TasksState>, task_run_id: Id) -> AppResult<TaskRunDetail> {
    state.scheduler.get_task_run(task_run_id).await
}
