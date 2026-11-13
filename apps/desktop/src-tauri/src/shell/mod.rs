//! The native shell around the webview: window material, theme, tray, global hotkey, the Dynamic
//! Island window, the first-run setup window, the macOS app menu, the spell checker, close and quit behaviour, haptics. Owner: Core agent.
//!
//! State lives in one managed [`Shell`]. It learns about recording and settings changes by
//! listening to backend events (see [`crate::events`]), so other areas never call into it.

pub mod haptics;
pub mod hotkey;
pub mod island;
pub mod material;
pub mod onboarding;
#[cfg(target_os = "macos")]
pub mod menu;
pub mod spelling;
pub mod titlebar;
pub mod tray;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use margin_domain::{AppEvent, Events, RecordingPhase, RecordingState, RecordingTrigger, Settings, ThemeMode, ToastKind, ToastRequest};
use margin_store::Db;
use tauri::{AppHandle, Manager, RunEvent, Theme, Window, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::core::Core;

/// Label of the main window (tauri.conf.json).
pub const MAIN: &str = "main";

/// Shell state, managed by Tauri. Reach it with `app.state::<Shell>()`.
pub struct Shell {
    db: Db,
    events: Events,
    /// The settings the shell last applied.
    settings: Mutex<Settings>,
    recording: Mutex<RecordingState>,
    main_focused: AtomicBool,
    /// Toasts raised before the UI was listening; flushed by `show_main_window`.
    pending_toasts: Mutex<Option<Vec<ToastRequest>>>,
    /// Set once the user confirmed quitting, so close and exit requests go through.
    quitting: AtomicBool,
    pub(crate) material: Mutex<material::State>,
    pub(crate) hotkey: Mutex<Option<hotkey::Registered>>,
    pub(crate) tray: Mutex<Option<tray::Items>>,
    pub(crate) island_visible: AtomicBool,
    /// Setup is closing; a second Open Margin or close does nothing until it is done.
    pub(crate) onboarding_finishing: AtomicBool,
}

impl Shell {
    fn new(db: Db, events: Events, settings: Settings) -> Self {
        Self {
            db,
            events,
            settings: Mutex::new(settings),
            recording: Mutex::new(RecordingState::idle()),
            main_focused: AtomicBool::new(false),
            pending_toasts: Mutex::new(Some(Vec::new())),
            quitting: AtomicBool::new(false),
            material: Mutex::new(material::State::default()),
            hotkey: Mutex::new(None),
            tray: Mutex::new(None),
            island_visible: AtomicBool::new(false),
            onboarding_finishing: AtomicBool::new(false),
        }
    }

    /// The settings as the shell last applied them.
    pub fn settings(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn recording_phase(&self) -> RecordingPhase {
        self.recording.lock().map(|r| r.phase).unwrap_or(RecordingPhase::Idle)
    }

    pub fn main_focused(&self) -> bool {
        self.main_focused.load(Ordering::Relaxed)
    }

    /// Raise a toast now, or as soon as the UI has painted.
    pub fn toast(&self, toast: ToastRequest) {
        if let Ok(mut pending) = self.pending_toasts.lock()
            && let Some(queue) = pending.as_mut()
        {
            queue.push(toast);
            return;
        }
        self.events.emit(AppEvent::Toast { toast });
    }

    /// The UI is listening: deliver queued toasts.
    pub fn ui_ready(&self) {
        let queued = self.pending_toasts.lock().ok().and_then(|mut p| p.take()).unwrap_or_default();
        for toast in queued {
            self.events.emit(AppEvent::Toast { toast });
        }
    }
}

/// Plain error toast.
pub fn error_toast(title: impl Into<String>, description: impl Into<String>) -> ToastRequest {
    ToastRequest { kind: ToastKind::Error, title: title.into(), description: Some(description.into()), action_label: None, action: None }
}

// ── Setup ──────────────────────────────────────────────────────────────────

/// Build the shell: theme and material on the main window, the island window, the tray, the
/// global hotkey, launch at login, and the event listener that keeps them in step.
pub fn setup(app: &AppHandle, core: &Core) -> margin_domain::AppResult<()> {
    let settings = tauri::async_runtime::block_on(core.db.get_settings())?;
    app.manage(Shell::new(core.db.clone(), core.events.clone(), settings.clone()));

    apply_theme(app, settings.appearance.theme);
    material::sync(app);
    if let Err(error) = island::create(app) {
        tracing::warn!(%error, "could not create the island window");
    }
    // First launch: setup gets its own window and the main window waits until it is done.
    if !settings.onboarding_done
        && let Err(error) = onboarding::create(app)
    {
        tracing::warn!(%error, "could not create the setup window; opening Margin instead");
    }
    if let Err(error) = tray::create(app, &settings) {
        tracing::warn!(%error, "could not create the tray icon");
    }
    hotkey::sync(app, &settings);
    // Keep the login item pointing at this copy of the app. Skipped in debug builds so a dev
    // run never registers a target/debug binary.
    if cfg!(not(debug_assertions)) {
        sync_autostart(app, settings.launch_at_login);
    }

    let handle = app.clone();
    core.on_event(move |event| on_app_event(&handle, event));
    app.on_menu_event(|app, event| {
        let id = event.id().as_ref();
        if !tray::on_menu_event(app, id) {
            #[cfg(target_os = "macos")]
            if id == menu::QUIT_ID {
                request_quit(app);
            }
        }
    });
    Ok(())
}

/// React to backend events: recording state drives the tray label and the island; settings
/// changes apply their side effects.
fn on_app_event(app: &AppHandle, event: &AppEvent) {
    match event {
        AppEvent::RecordingStateChanged { state } => {
            let shell = app.state::<Shell>();
            if let Ok(mut current) = shell.recording.lock() {
                *current = state.clone();
            }
            tray::set_recording(app, state.phase);
            island::sync(app);
        }
        AppEvent::SettingsChanged { settings } => apply_settings(app, settings),
        _ => {}
    }
}

/// Apply what changed between the settings the shell last saw and `new`.
fn apply_settings(app: &AppHandle, new: &Settings) {
    let shell = app.state::<Shell>();
    let old = match shell.settings.lock() {
        Ok(mut current) => std::mem::replace(&mut *current, new.clone()),
        Err(_) => return,
    };
    if old.appearance.theme != new.appearance.theme {
        apply_theme(app, new.appearance.theme);
    }
    if old.appearance != new.appearance {
        material::sync(app);
    }
    if old.launch_at_login != new.launch_at_login {
        sync_autostart(app, new.launch_at_login);
    }
    if hotkey::accelerator(&old) != hotkey::accelerator(new) {
        hotkey::sync(app, new);
    }
    if old.recording.auto_detect != new.recording.auto_detect {
        tray::set_listening(app, new.recording.auto_detect);
    }
}

/// Persist settings changed by the shell itself (the tray's Listening item) and tell everyone.
pub fn save_settings(app: &AppHandle, change: impl FnOnce(&mut Settings) + Send + 'static) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let shell = app.state::<Shell>();
        let result = async {
            let mut settings = shell.db.get_settings().await?;
            change(&mut settings);
            shell.db.set_settings(&settings).await?;
            shell.db.get_settings().await
        }
        .await;
        match result {
            Ok(settings) => shell.events.emit(AppEvent::SettingsChanged { settings }),
            Err(error) => shell.toast(error_toast("Settings were not saved", margin_domain::AppError::from(error).message)),
        }
    });
}

/// The native window theme for a setting (`None` follows the system).
pub(crate) fn window_theme(mode: ThemeMode) -> Option<Theme> {
    match mode {
        ThemeMode::System => None,
        ThemeMode::Light => Some(Theme::Light),
        ThemeMode::Dark => Some(Theme::Dark),
    }
}

/// The main and setup windows follow the theme setting; the island is always dark.
fn apply_theme(app: &AppHandle, mode: ThemeMode) {
    let theme = window_theme(mode);
    for label in [MAIN, onboarding::LABEL] {
        if let Some(window) = app.get_webview_window(label)
            && let Err(error) = window.set_theme(theme)
        {
            tracing::warn!(%error, label, "could not set the window theme");
        }
    }
}

fn sync_autostart(app: &AppHandle, enabled: bool) {
    let autostart = app.autolaunch();
    let result = if enabled { autostart.enable() } else { autostart.disable() };
    if let Err(error) = result {
        tracing::warn!(%error, enabled, "could not change launch at login");
        app.state::<Shell>().toast(error_toast(
            if enabled { "Margin could not turn on Open at login" } else { "Margin could not turn off Open at login" },
            "Check Login Items in your system settings and try again.",
        ));
    }
}

// ── Windows, close and quit ────────────────────────────────────────────────

/// Show, unminimise and focus the main window. While setup is open, bring setup forward instead.
pub fn show_main(app: &AppHandle) {
    if onboarding::is_open(app) {
        if let Err(error) = onboarding::show(app) {
            tracing::warn!(message = %error.message, "could not show the setup window");
        }
        return;
    }
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// True while something must not be cut short: capture, or the transcription and summary after it.
pub fn is_busy(app: &AppHandle) -> bool {
    app.state::<Shell>().recording_phase() != RecordingPhase::Idle || crate::commands::recording::is_recording(app)
}

/// True while audio is being captured.
pub fn is_capturing(app: &AppHandle) -> bool {
    matches!(app.state::<Shell>().recording_phase(), RecordingPhase::Starting | RecordingPhase::Recording)
}

/// `Builder::on_window_event` handler.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    // Closing setup early opens Margin with whatever was set up.
    if window.label() == onboarding::LABEL {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let app = window.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = onboarding::finish(&app).await {
                    app.state::<Shell>().toast(error_toast("Setup was not saved", error.message));
                }
            });
        }
        return;
    }
    if window.label() != MAIN {
        return;
    }
    let app = window.app_handle();
    let Some(shell) = app.try_state::<Shell>() else { return };
    match event {
        WindowEvent::CloseRequested { api, .. } => {
            if shell.quitting.load(Ordering::SeqCst) {
                return;
            }
            api.prevent_close();
            // Stay in the tray while recording or listening for meetings; otherwise quit.
            if is_busy(app) || shell.settings().recording.auto_detect {
                let _ = window.hide();
            } else {
                request_quit(app);
            }
        }
        WindowEvent::Focused(focused) => {
            shell.main_focused.store(*focused, Ordering::Relaxed);
            island::sync(app);
            if *focused {
                // Picks up a Reduce transparency change made while Margin was in the background.
                material::sync(app);
            }
        }
        WindowEvent::ThemeChanged(_) => material::sync(app),
        _ => {}
    }
}

/// `App::run` handler.
pub fn on_run_event(app: &AppHandle, event: RunEvent) {
    match event {
        RunEvent::ExitRequested { code: None, api, .. } => {
            if let Some(shell) = app.try_state::<Shell>()
                && !shell.quitting.load(Ordering::SeqCst)
                && is_busy(app)
            {
                api.prevent_exit();
                request_quit(app);
            }
        }
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => show_main(app),
        _ => {}
    }
}

/// Quit, asking first while a recording is running (Cmd+Q, tray Quit, closing the window).
pub fn request_quit(app: &AppHandle) {
    if !is_busy(app) {
        quit_now(app);
        return;
    }
    let app = app.clone();
    app.dialog()
        .message("Margin is still recording or writing up this meeting. Quitting stops it now; the audio captured so far stays on this computer.")
        .title("Stop recording and quit?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Quit".into(), "Keep recording".into()))
        .show({
            let app = app.clone();
            move |quit| {
                if quit {
                    quit_now(&app);
                }
            }
        });
}

/// Stop capture if it is running (giving it a few seconds to flush), then exit.
fn quit_now(app: &AppHandle) {
    let shell = app.state::<Shell>();
    if shell.quitting.swap(true, Ordering::SeqCst) {
        return;
    }
    // Let the UI flush unsaved edits before the process goes away.
    if let Some(core) = app.try_state::<Core>() {
        core.events.emit(AppEvent::WillQuit);
    }
    let capturing = is_capturing(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        if !capturing {
            app.exit(0);
            return;
        }
        let stop = crate::commands::recording::toggle_recording(&app, RecordingTrigger::Manual);
        if let Err(error) = tokio::time::timeout(Duration::from_secs(5), stop).await.unwrap_or(Ok(RecordingState::idle())).map(|_| ()) {
            tracing::warn!(message = %error.message, "could not stop the recording before quitting");
        }
        app.exit(0);
    });
}

/// Start or stop recording from the shell (tray, hotkey). Failures become a toast.
pub fn toggle_recording(app: &AppHandle, trigger: RecordingTrigger) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let was_capturing = is_capturing(&app);
        if let Err(error) = crate::commands::recording::toggle_recording(&app, trigger).await {
            let title = if was_capturing { "Recording did not stop" } else { "Recording did not start" };
            app.state::<Shell>().toast(error_toast(title, error.message));
        }
    });
}
