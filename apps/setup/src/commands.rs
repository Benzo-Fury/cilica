//! What the window can ask Setup to do. Reaches the UI through the generated
//! `apps/desktop/src/ipc/installer/bindings.ts` (`bun run bindings`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cilica_domain::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::job::{Busy, Reporter};
use crate::plan::{InstallOptions, Launch, Mode, Step, UninstallOptions, expected_install_bytes, install_dir_for, install_steps, megabytes, uninstall_steps};
use crate::{WINDOW, payload, system};

/// Setup's state for this run.
pub struct Setup {
    pub launch: Launch,
    /// This run's temporary folder (see [`system::session_dir`]).
    pub session: PathBuf,
    pub busy: Busy,
    /// Cilica's program file, once installed.
    installed: Mutex<Option<PathBuf>>,
}

impl Setup {
    pub fn new(launch: Launch, session: PathBuf) -> Self {
        Self { launch, session, busy: Busy::default(), installed: Mutex::new(None) }
    }
}

/// Cilica, already installed.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct InstalledCilica {
    pub version: String,
    pub dir: String,
}

/// Everything the first screen needs.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SetupInfo {
    pub mode: Mode,
    /// The version Setup installs.
    pub version: String,
    /// Where Cilica goes when nobody picks a folder.
    pub default_dir: String,
    /// Cilica already on this PC. Installing updates it in place.
    pub existing: Option<InstalledCilica>,
    /// Where Cilica keeps notes, recordings and settings.
    pub data_dir: String,
    /// Cilica's size once installed, in whole megabytes.
    pub size_mb: u32,
    /// Cilica is open; the installer or uninstaller will close it.
    pub cilica_running: bool,
}

fn text(path: &Path) -> String {
    path.display().to_string()
}

#[tauri::command]
#[specta::specta]
pub fn get_setup_info(state: State<'_, Setup>) -> SetupInfo {
    let existing = system::existing();
    let size = expected_install_bytes(payload::app_bytes(), payload::INSTALLER.len() as u64);
    SetupInfo {
        mode: state.launch.mode,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        default_dir: text(&system::default_install_dir()),
        cilica_running: system::cilica_running(existing.as_ref().map(|e| e.dir.as_path())),
        existing: existing.map(|e| InstalledCilica { version: e.version, dir: text(&e.dir) }),
        data_dir: text(&system::data_dir()),
        size_mb: megabytes(size),
    }
}

/// Show the window; the UI calls this after its first paint so it never flashes white.
#[tauri::command]
#[specta::specta]
pub fn show_window(app: AppHandle) -> AppResult<()> {
    let window = app.get_webview_window(WINDOW).ok_or_else(|| AppError::internal("Setup's window is missing. Open Setup again."))?;
    window
        .show()
        .and_then(|()| window.set_focus())
        .map_err(|e| AppError::internal(format!("Setup couldn't show its window: {e}. Open Setup again.")))
}

/// Pick where Cilica goes. Returns the folder it would be installed in (a `Cilica` folder inside
/// the one picked), or nothing when the picker was closed.
#[tauri::command]
#[specta::specta]
pub async fn choose_folder(app: AppHandle, current: String) -> AppResult<Option<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app.dialog().file().set_title("Choose where to install Cilica");
    if let Some(parent) = Path::new(&current).parent().filter(|p| p.is_dir()) {
        dialog = dialog.set_directory(parent);
    }
    if let Some(window) = app.get_webview_window(WINDOW) {
        dialog = dialog.set_parent(&window);
    }
    dialog.pick_folder(move |picked| {
        let _ = tx.send(picked);
    });
    let picked = rx.await.map_err(|_| AppError::internal("The folder picker closed unexpectedly. Try again."))?;
    let Some(picked) = picked else { return Ok(None) };
    let path = picked.into_path().map_err(|e| AppError::invalid(format!("That folder can't be used: {e}. Choose a folder on this PC.")))?;
    Ok(Some(text(&install_dir_for(&path))))
}

/// Start installing. Returns the steps the progress screen lists; progress follows as events.
#[tauri::command]
#[specta::specta]
pub fn install(app: AppHandle, state: State<'_, Setup>, options: InstallOptions) -> AppResult<Vec<Step>> {
    if state.launch.mode != Mode::Install {
        return Err(AppError::invalid("This copy of Setup only removes Cilica. Download Cilica to install it."));
    }
    // An update goes where Cilica already is; a second copy elsewhere would confuse its updater.
    let mut options = options;
    let updating = match system::existing() {
        Some(existing) => {
            options.dir = text(&existing.dir);
            true
        }
        None => false,
    };
    if options.dir.trim().is_empty() {
        return Err(AppError::invalid("Choose a folder for Cilica."));
    }
    let steps = install_steps(&options, updating);
    start(&app, &state, steps.clone(), move |setup, report| {
        let exe = system::install(&options, updating, &setup.session, report)?;
        if let Ok(mut installed) = setup.installed.lock() {
            *installed = Some(exe);
        }
        Ok(())
    })?;
    Ok(steps)
}

/// Start removing Cilica. Returns the steps; progress follows as events.
#[tauri::command]
#[specta::specta]
pub fn uninstall(app: AppHandle, state: State<'_, Setup>, options: UninstallOptions) -> AppResult<Vec<Step>> {
    let existing = system::existing().ok_or_else(|| AppError::not_found("Cilica isn't installed on this PC, so there's nothing to remove."))?;
    let steps = uninstall_steps(options);
    start(&app, &state, steps.clone(), move |_, report| system::uninstall(&existing.dir, options, report))?;
    Ok(steps)
}

/// Run `job` off the main thread, reporting as it goes. One job at a time.
fn start(
    app: &AppHandle,
    state: &Setup,
    steps: Vec<Step>,
    job: impl FnOnce(&Setup, &Reporter) -> AppResult<()> + Send + 'static,
) -> AppResult<()> {
    if !state.busy.start() {
        return Err(AppError::new(ErrorCode::Conflict, "Setup is already working. Wait for it to finish."));
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let setup = app.state::<Setup>();
        let report = Reporter::new(app.clone(), steps);
        match job(&setup, &report) {
            Ok(()) => report.finished(),
            Err(error) => report.failed(error),
        }
        setup.busy.stop();
    });
    Ok(())
}

/// Open the Cilica just installed, then close Setup.
#[tauri::command]
#[specta::specta]
pub fn open_cilica(app: AppHandle, state: State<'_, Setup>) -> AppResult<()> {
    let exe = state
        .installed
        .lock()
        .ok()
        .and_then(|i| i.clone())
        .or_else(|| system::existing().map(|e| e.dir.join("Cilica.exe")))
        .ok_or_else(|| AppError::not_found("Cilica isn't installed yet. Install it first."))?;
    system::open(&exe)?;
    app.exit(0);
    Ok(())
}

/// Close Setup. Refused while a job runs: stopping the installer halfway leaves Cilica broken.
#[tauri::command]
#[specta::specta]
pub fn quit(app: AppHandle, state: State<'_, Setup>) -> AppResult<()> {
    if state.busy.get() {
        return Err(AppError::new(ErrorCode::Conflict, "Setup is still working. Wait for it to finish, then close it."));
    }
    app.exit(0);
    Ok(())
}
