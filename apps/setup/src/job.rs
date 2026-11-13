//! A running install or uninstall: the one event the window listens to, and the reporter the
//! platform code calls as each step moves. The reporter also fills the taskbar button, so the
//! progress shows while the window is behind others or minimised.

use std::sync::atomic::{AtomicBool, Ordering};

use cilica_domain::AppError;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::WINDOW;
use crate::plan::{Step, overall};

/// What a job reports. The window listens once and switches on `type`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    /// `step` started or moved. `progress` is the whole job, 0..1.
    Step { step: Step, progress: f32 },
    /// `step` finished.
    StepDone { step: Step, progress: f32 },
    /// Every step finished.
    Finished,
    /// The job stopped. Steps already done stay done.
    Failed { error: AppError },
}

/// The single Setup → window event.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct SetupEvent(pub JobEvent);

/// Whether a job is running. Closing the window waits for it.
#[derive(Default)]
pub struct Busy(AtomicBool);

impl Busy {
    pub fn get(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Mark a job started. False if one already was.
    pub fn start(&self) -> bool {
        !self.0.swap(true, Ordering::SeqCst)
    }

    pub fn stop(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Handed to the platform code. Cheap to call often: the window smooths the bar itself.
pub struct Reporter {
    app: AppHandle,
    steps: Vec<Step>,
}

impl Reporter {
    pub fn new(app: AppHandle, steps: Vec<Step>) -> Self {
        Self { app, steps }
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// `step` is `within` (0..1) done.
    pub fn progress(&self, step: Step, within: f32) {
        let progress = overall(&self.steps, step, within);
        self.taskbar(ProgressBarStatus::Normal, Some(progress));
        self.emit(JobEvent::Step { step, progress });
    }

    pub fn start(&self, step: Step) {
        self.progress(step, 0.0);
    }

    pub fn done(&self, step: Step) {
        let progress = overall(&self.steps, step, 1.0);
        self.taskbar(ProgressBarStatus::Normal, Some(progress));
        self.emit(JobEvent::StepDone { step, progress });
    }

    pub fn finished(&self) {
        self.taskbar(ProgressBarStatus::None, None);
        self.emit(JobEvent::Finished);
    }

    /// The taskbar button turns red and stays at its last value until the window closes.
    pub fn failed(&self, error: AppError) {
        tracing::warn!(error = %error.message, "setup stopped");
        self.taskbar(ProgressBarStatus::Error, None);
        self.emit(JobEvent::Failed { error });
    }

    fn emit(&self, event: JobEvent) {
        if let Err(error) = SetupEvent(event).emit(&self.app) {
            tracing::warn!(%error, "could not send setup progress to the window");
        }
    }

    fn taskbar(&self, status: ProgressBarStatus, progress: Option<f32>) {
        let Some(window) = self.app.get_webview_window(WINDOW) else { return };
        let state = ProgressBarState { status: Some(status), progress: progress.map(|p| (p * 100.0).round() as u64) };
        if let Err(error) = window.set_progress_bar(state) {
            tracing::debug!(%error, "could not set taskbar progress");
        }
    }
}
