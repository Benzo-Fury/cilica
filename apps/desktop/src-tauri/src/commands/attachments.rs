//! Images in notes: pasting, dropping, picking and downloading them into the attachments folder
//! (`<app data>/attachments/`), and keeping that folder tidy. The work lives in
//! `cilica-attachments`; this module wires it to commands, settings, events and the OS.
//!
//! At startup, and after every import, inline `data:` images left in note bodies and versions are
//! moved out into files. At startup, files nothing points at (and older than a day) are removed.

use std::path::PathBuf;
use std::time::Duration;

use cilica_attachments::{FOLDER, HeicConverter, ImageStore, NoteImages, SaveOptions};
use cilica_domain::*;
use tauri::{AppHandle, Manager, State};

use crate::commands::image_text;
use crate::core::Core;

/// Attachments younger than this are never cleaned up, so an image pasted moments ago survives
/// until its note saves.
const KEEP_UNUSED_FOR: Duration = Duration::from_secs(24 * 60 * 60);

/// Build the attachments store, manage it, and start the startup tidy-up and the after-import pass.
pub fn setup(app: &AppHandle, core: &Core) -> AppResult<()> {
    // The folder and the store, with the OS's HEIC converter where there is one
    let store = ImageStore::new(core.data_dir.join(FOLDER)).with_heic_converter(heic_converter());
    store.ensure_dir()?;
    let images = NoteImages::new(core.db.clone(), store);
    app.manage(images.clone());

    // Startup: move inline images out, then remove files nothing uses
    let startup = Tidy { images: images.clone(), events: core.events.clone() };
    tauri::async_runtime::spawn(async move {
        startup.externalize().await;
        match startup.images.clean_up(KEEP_UNUSED_FOR).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(files = n, "removed unused attachments"),
            Err(error) => tracing::warn!(%error, "could not clean up attachments"),
        }
    });

    // After an import finishes: imported notes carry their images inline
    let after_import = Tidy { images, events: core.events.clone() };
    core.on_event(move |event| {
        if let AppEvent::ImportProgress { progress } = event
            && progress.phase == ImportPhase::Done
        {
            let tidy = after_import.clone();
            tauri::async_runtime::spawn(async move { tidy.externalize().await });
        }
    });
    Ok(())
}

/// The background passes over the whole database.
#[derive(Clone)]
struct Tidy {
    images: NoteImages,
    events: Events,
}

impl Tidy {
    /// Move every inline image out of note bodies and versions, and tell the UI which notes changed.
    async fn externalize(&self) {
        match self.images.externalize_all().await {
            Ok(report) => {
                if report.images > 0 || report.failed > 0 {
                    tracing::info!(notes = report.notes.len(), versions = report.versions, images = report.images, failed = report.failed, "moved inline images to attachments");
                }
                if !report.notes.is_empty() || report.versions > 0 {
                    let note_id = if report.notes.len() == 1 { report.notes.first().copied() } else { None };
                    self.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Notes, DataScope::Versions], note_id });
                }
            }
            Err(error) => tracing::warn!(%error, "could not move inline images to attachments"),
        }
    }
}

/// How images are saved right now (Settings › Writing).
async fn save_options(core: &Core) -> AppResult<SaveOptions> {
    Ok(SaveOptions { shrink_large: core.db.get_settings().await?.writing.shrink_large_images })
}

// ---------- Commands ---------- //

/// Save a pasted image (base64, with or without a `data:` prefix). `name` suggests the alt text.
#[tauri::command]
#[specta::specta]
pub async fn save_image(app: AppHandle, core: State<'_, Core>, images: State<'_, NoteImages>, data: String, name: Option<String>) -> AppResult<SavedImage> {
    let options = save_options(&core).await?;
    let saved = images.store().save_base64(&data, name.as_deref(), options).await?;
    image_text::wake(&app);
    Ok(saved)
}

/// Save an image file dropped from Finder or picked in the file picker.
#[tauri::command]
#[specta::specta]
pub async fn import_image_file(app: AppHandle, core: State<'_, Core>, images: State<'_, NoteImages>, path: String) -> AppResult<SavedImage> {
    if path.trim().is_empty() {
        return Err(AppError::invalid("Choose an image first."));
    }
    let options = save_options(&core).await?;
    let saved = images.store().save_file(&PathBuf::from(path), options).await?;
    image_text::wake(&app);
    Ok(saved)
}

/// Download an image from a web link and save it. Only public http and https sites are reached.
#[tauri::command]
#[specta::specta]
pub async fn fetch_image(app: AppHandle, core: State<'_, Core>, images: State<'_, NoteImages>, url: String) -> AppResult<SavedImage> {
    let options = save_options(&core).await?;
    let saved = images.store().fetch(&url, options).await?;
    image_text::wake(&app);
    Ok(saved)
}

/// The attachments folder as an absolute path, so the UI can turn `attachments/<name>` into an
/// asset URL.
#[tauri::command]
#[specta::specta]
pub async fn attachments_dir(images: State<'_, NoteImages>) -> AppResult<String> {
    Ok(images.store().dir().to_string_lossy().into_owned())
}

/// Save a copy of an attachment (`attachments/<name>`) to `dest` (Save as…).
#[tauri::command]
#[specta::specta]
pub async fn save_image_copy(images: State<'_, NoteImages>, path: String, dest: String) -> AppResult<()> {
    if dest.trim().is_empty() {
        return Err(AppError::invalid("Choose where to save the image."));
    }
    images.store().copy_to(&path, &PathBuf::from(dest)).await
}

// ---------- HEIC ---------- //

/// The OS's HEIC → JPEG converter: `sips` on macOS, none elsewhere.
fn heic_converter() -> Option<HeicConverter> {
    #[cfg(target_os = "macos")]
    {
        Some(std::sync::Arc::new(sips::to_jpeg))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// HEIC conversion through macOS's built-in `sips`.
#[cfg(target_os = "macos")]
mod sips {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use cilica_domain::{AppError, AppResult};

    /// Longest a conversion may take before it is stopped.
    const TIMEOUT: Duration = Duration::from_secs(30);

    /// Write the bytes to a private temporary file, convert it with `sips`, read the JPEG back.
    pub fn to_jpeg(heic: &[u8]) -> AppResult<Vec<u8>> {
        let failed = || AppError::invalid("Cilica couldn't convert that HEIC image. Save it as JPEG or PNG and try again.");

        // A private folder for the input and output
        let dir = std::env::temp_dir().join(format!("cilica-heic-{}-{}", std::process::id(), cilica_domain::new_id()));
        std::fs::create_dir_all(&dir).map_err(|_| failed())?;
        let (input, output) = (dir.join("in.heic"), dir.join("out.jpg"));
        let result = (|| {
            std::fs::write(&input, heic).map_err(|_| failed())?;

            // Run sips with a deadline
            let mut child = Command::new("/usr/bin/sips")
                .args(["-s", "format", "jpeg"])
                .arg(&input)
                .arg("--out")
                .arg(&output)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| failed())?;
            let started = Instant::now();
            let status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) if started.elapsed() < TIMEOUT => std::thread::sleep(Duration::from_millis(50)),
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(failed());
                    }
                }
            };
            if !status.success() {
                return Err(failed());
            }
            std::fs::read(&output).map_err(|_| failed())
        })();

        // Always remove the temporary files
        let _ = std::fs::remove_dir_all(&dir);
        result
    }
}
