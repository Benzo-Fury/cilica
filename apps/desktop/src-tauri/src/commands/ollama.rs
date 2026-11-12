//! Local models through Ollama: install and start Ollama if needed, pull, delete and choose a model.
//! Owner: AI agent.
//!
//! Model downloads report through `AppEvent::ModelDownload` with ids `ollama:<tag>`; installing Ollama
//! itself reports as `ollama:app`. The HTTP side lives in `margin_llm::ollama`; finding, unpacking and
//! launching the Ollama app is platform-specific and stays here.

use std::path::PathBuf;
use std::time::Duration;

use margin_domain::*;
use margin_llm::ollama::{self, OllamaManager};
use tauri::{AppHandle, Manager, State};

use crate::core::Core;

/// Managed state for this area.
pub struct OllamaState {
    manager: OllamaManager,
}

pub fn setup(app: &AppHandle, core: &Core) -> AppResult<()> {
    app.manage(OllamaState { manager: OllamaManager::new(core.events.clone()) });
    Ok(())
}

async fn root(core: &Core) -> String {
    let ai = core.db.ai_settings().await.unwrap_or_default();
    ollama::root_url(ai.base_url.as_deref())
}

// ── Finding and starting Ollama ───────────────────────────────────────────

#[cfg(target_os = "macos")]
const INSTALLER_URL: &str = "https://ollama.com/download/Ollama-darwin.zip";

/// Where an Ollama app bundle or CLI might live.
fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(target_os = "macos")]
    {
        out.push(PathBuf::from("/Applications/Ollama.app"));
        if let Some(home) = std::env::var_os("HOME") {
            out.push(PathBuf::from(home).join("Applications/Ollama.app"));
        }
        out.push(PathBuf::from("/opt/homebrew/bin/ollama"));
        out.push(PathBuf::from("/usr/local/bin/ollama"));
    }
    #[cfg(target_os = "linux")]
    {
        out.push(PathBuf::from("/usr/local/bin/ollama"));
        out.push(PathBuf::from("/usr/bin/ollama"));
    }
    #[cfg(target_os = "windows")]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        out.push(PathBuf::from(local).join("Programs/Ollama/ollama.exe"));
    }
    out
}

fn find_ollama() -> Option<PathBuf> {
    candidates().into_iter().find(|p| p.exists())
}

/// Start the app bundle, or `ollama serve` for a bare CLI.
fn launch(path: &PathBuf) -> AppResult<()> {
    let spawned = if path.extension().is_some_and(|e| e == "app") {
        std::process::Command::new("open").arg("-g").arg(path).spawn()
    } else {
        std::process::Command::new(path)
            .arg("serve")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
    };
    spawned.map(|_| ()).map_err(|_| AppError::unavailable("Margin could not start Ollama. Open it yourself, then try again."))
}

async fn wait_until_running(manager: &OllamaManager, root: &str) -> bool {
    for _ in 0..40 {
        if manager.version(root).await.is_some() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

/// Download Ollama into ~/Applications and unpack it. macOS only; elsewhere people install it themselves.
#[cfg(target_os = "macos")]
async fn install(manager: &OllamaManager, data_dir: &std::path::Path) -> AppResult<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| AppError::unavailable("Margin could not find your home folder."))?;
    let apps = PathBuf::from(home).join("Applications");
    let _ = std::fs::create_dir_all(&apps);
    let zip = data_dir.join("Ollama-darwin.zip");
    manager.download_installer(INSTALLER_URL, &zip).await?;
    let status = tokio::process::Command::new("ditto").arg("-x").arg("-k").arg(&zip).arg(&apps).status().await;
    let _ = std::fs::remove_file(&zip);
    match status {
        Ok(s) if s.success() => Ok(apps.join("Ollama.app")),
        _ => Err(AppError::unavailable("Margin downloaded Ollama but could not unpack it. Try again.")),
    }
}

#[cfg(not(target_os = "macos"))]
async fn install(_manager: &OllamaManager, _data_dir: &std::path::Path) -> AppResult<PathBuf> {
    Err(AppError::unavailable("Install Ollama from ollama.com, then try again."))
}

/// Make sure Ollama is installed and answering, installing and starting it as needed.
async fn ensure_running(manager: &OllamaManager, core: &Core, root: &str) -> AppResult<()> {
    if manager.version(root).await.is_some() {
        return Ok(());
    }
    if !ollama::is_local_root(root) {
        return Err(AppError::unavailable(format!("Margin could not reach Ollama at {root}. Start it there, then try again.")));
    }
    let path = match find_ollama() {
        Some(p) => p,
        None => install(manager, &core.data_dir).await?,
    };
    launch(&path)?;
    if wait_until_running(manager, root).await {
        Ok(())
    } else {
        Err(AppError::unavailable("Ollama did not start. Open it yourself, then try again."))
    }
}

/// Point Margin at Ollama and the given model, for both the assistant and quick tasks.
async fn use_model(core: &Core, model: &str) -> AppResult<()> {
    let mut settings = core.db.get_settings().await?;
    settings.ai.provider = ProviderKind::Ollama;
    settings.ai.model = model.to_string();
    settings.ai.fast_model = model.to_string();
    core.db.set_settings(&settings).await?;
    let saved = core.db.get_settings().await?;
    core.events.emit(AppEvent::SettingsChanged { settings: saved });
    Ok(())
}

// ── Commands ─────────────────────────────────────────────────────────────

#[tauri::command]
#[specta::specta]
pub async fn get_ollama_status(core: State<'_, Core>, state: State<'_, OllamaState>) -> AppResult<OllamaStatus> {
    let root = root(&core).await;
    let version = state.manager.version(&root).await;
    Ok(OllamaStatus { installed: version.is_some() || find_ollama().is_some(), running: version.is_some(), version })
}

/// The models Margin offers, marked with what Ollama already has.
#[tauri::command]
#[specta::specta]
pub async fn list_local_models(core: State<'_, Core>, state: State<'_, OllamaState>) -> AppResult<Vec<LocalLlmModel>> {
    Ok(state.manager.list(&root(&core).await).await)
}

/// Install and start Ollama if needed. Downloads no model: the person picks one afterwards.
/// Returns at once; progress and the outcome arrive as `ollama:app` events.
#[tauri::command]
#[specta::specta]
pub async fn set_up_ollama(app: AppHandle) -> AppResult<()> {
    tauri::async_runtime::spawn(async move {
        let core = app.state::<Core>();
        let manager = app.state::<OllamaState>().manager.clone();
        let root = root(&core).await;
        // Always end with an `ollama:app` event, done or failed, so the page never waits for ever.
        match ensure_running(&manager, &core, &root).await {
            Ok(()) => manager.finish_install(None),
            Err(e) => {
                tracing::warn!(message = %e.message, "could not set up Ollama");
                manager.finish_install(Some(e.message));
            }
        }
    });
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn pull_local_model(core: State<'_, Core>, state: State<'_, OllamaState>, model_id: String) -> AppResult<()> {
    let root = root(&core).await;
    ensure_running(&state.manager, &core, &root).await?;
    state.manager.start_pull(root, model_id)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_local_model_pull(state: State<'_, OllamaState>, model_id: String) -> AppResult<()> {
    state.manager.cancel(&model_id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_local_model(core: State<'_, Core>, state: State<'_, OllamaState>, model_id: String) -> AppResult<()> {
    state.manager.delete(&root(&core).await, &model_id).await
}

/// Use a pulled model for the assistant and quick tasks.
#[tauri::command]
#[specta::specta]
pub async fn select_local_model(core: State<'_, Core>, model_id: String) -> AppResult<()> {
    use_model(&core, &model_id).await
}
