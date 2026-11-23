//! Settings, app info, windows (main and first-run setup), haptics. Owner: Core agent.
//!
//! Every command here is part of the IPC contract in `src/ipc/bindings.ts`. Keep names, arguments and
//! return types as they are unless the orchestrator agrees to a change.

use cilica_domain::*;
use tauri::{AppHandle, Manager, State};

use crate::core::Core;
pub use crate::shell::haptics::HapticKind;
use crate::shell::{self, Shell, material, onboarding};

/// Facts about this build and machine that the UI needs to pick its chrome.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct AppInfo {
    pub version: String,
    /// "macos", "windows" or "linux".
    pub os: String,
    pub arch: String,
    /// The window has a native see-through material (vibrancy, Liquid Glass, Mica).
    pub has_window_material: bool,
    /// Absolute path of the app data directory.
    pub data_dir: String,
}

/// Called once at startup, after the database is open: builds the shell (theme, window material,
/// tray, taskbar, global hotkey).
pub fn setup(app: &AppHandle, core: &Core) -> AppResult<()> {
    shell::setup(app, core)
}

#[tauri::command]
#[specta::specta]
pub async fn get_settings(core: State<'_, Core>) -> AppResult<Settings> {
    Ok(core.db.get_settings().await?)
}

/// Replace the settings document. Applies side effects (theme, window material, autostart, global hotkey) and emits SettingsChanged.
#[tauri::command]
#[specta::specta]
pub async fn update_settings(core: State<'_, Core>, settings: Settings) -> AppResult<Settings> {
    core.db.set_settings(&settings).await?;
    let saved = core.db.get_settings().await?;
    // The shell listens for this and applies the side effects (see `shell::apply_settings`).
    core.events.emit(AppEvent::SettingsChanged { settings: saved.clone() });
    Ok(saved)
}

#[tauri::command]
#[specta::specta]
pub async fn get_app_info(app: AppHandle, core: State<'_, Core>) -> AppResult<AppInfo> {
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        has_window_material: material::has_material(&app),
        data_dir: core.data_dir.to_string_lossy().into_owned(),
    })
}

/// The window starts hidden; the UI calls this after its first paint to avoid a white flash.
/// Also delivers toasts raised before the UI was listening.
#[tauri::command]
#[specta::specta]
pub async fn show_main_window(app: AppHandle) -> AppResult<()> {
    // First launch: the main window waits behind setup; `finish_onboarding` shows it.
    if onboarding::is_open(&app) && !app.state::<Shell>().settings().onboarding_done {
        return Ok(());
    }
    let window = app.get_webview_window(shell::MAIN).ok_or_else(|| AppError::internal("The main window is missing. Restart Cilica."))?;
    window.show().and_then(|_| window.set_focus()).map_err(|e| AppError::internal(format!("Cilica could not show its window: {e}")))?;
    app.state::<Shell>().ui_ready();
    Ok(())
}

/// The setup window starts hidden; its UI calls this after the first paint.
#[tauri::command]
#[specta::specta]
pub async fn show_onboarding_window(app: AppHandle) -> AppResult<()> {
    onboarding::show(&app)
}

/// Open Cilica from setup: marks setup done, writes the welcome note on the first run, closes
/// the setup window and shows the main window on that note.
#[tauri::command]
#[specta::specta]
pub async fn finish_onboarding(app: AppHandle) -> AppResult<()> {
    onboarding::finish(&app).await
}

/// Settings › General › Run setup again: open the setup window over Cilica.
#[tauri::command]
#[specta::specta]
pub async fn open_onboarding(app: AppHandle) -> AppResult<()> {
    if onboarding::is_open(&app) {
        return onboarding::show(&app);
    }
    onboarding::create(&app).map(|_| ()).map_err(|e| AppError::internal(format!("Cilica could not open setup: {e}. Restart Cilica and try again.")))
}

/// The user double-clicked an empty part of the title band: do what the system setting for
/// title-bar double-clicks asks (fill, zoom, minimise or nothing), with the system's animation.
#[tauri::command]
#[specta::specta]
pub async fn title_bar_double_click(window: tauri::WebviewWindow) -> AppResult<()> {
    shell::titlebar::double_click(&window).map_err(|e| AppError::internal(format!("Cilica could not resize its window: {e}. Try the green window button instead.")))
}

/// Trackpad feedback on macOS (`alignment`, `generic`, `level_change`); does nothing elsewhere.
#[tauri::command]
#[specta::specta]
pub async fn perform_haptic(app: AppHandle, kind: HapticKind) -> AppResult<()> {
    shell::haptics::perform(&app, kind);
    Ok(())
}
