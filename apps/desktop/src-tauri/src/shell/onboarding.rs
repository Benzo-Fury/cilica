//! First-run setup: a small window of its own (560 × 700) that walks through permissions, the
//! speech model, the assistant's model, imports and appearance (docs/spec.md › First launch). It
//! loads the same frontend with `?window=onboarding` (its label is also `onboarding`), is created
//! hidden and shows itself after its first paint.
//!
//! While it is open the main window stays hidden. Finishing or closing it marks setup done,
//! writes the welcome note (first run only), shows the main window on that note and closes setup.
//! Settings › General › Run setup again opens it over a visible main window.

use std::sync::atomic::Ordering;

use margin_domain::{AppError, AppEvent, AppResult, DataScope};
use margin_store::ShortcutStyle;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::Shell;

pub const LABEL: &str = "onboarding";
/// Logical size in points. Fixed: the layout and background art are drawn for it.
const SIZE: (f64, f64) = (560.0, 700.0);

/// Whether the setup window exists (it may still be hidden while its UI loads).
pub fn is_open(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL).is_some()
}

/// Build the setup window, hidden, or return the one already open.
pub fn create(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(LABEL) {
        return Ok(window);
    }
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?window=onboarding".into()))
        .title("Set up Margin")
        .inner_size(SIZE.0, SIZE.1)
        .resizable(false)
        .maximizable(false)
        .minimizable(true)
        .center()
        .visible(false)
        .theme(super::window_theme(app.state::<Shell>().settings().appearance.theme));
    // macOS keeps its traffic lights over the art; Windows and Linux get Margin's own close button.
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(tauri::LogicalPosition::new(18.0, 24.0));
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false);
    builder.build()
}

/// Show and focus the setup window (its UI calls this after the first paint).
pub fn show(app: &AppHandle) -> AppResult<()> {
    let window = app.get_webview_window(LABEL).ok_or_else(|| AppError::internal("The setup window is missing. Restart Margin."))?;
    window
        .unminimize()
        .and_then(|_| window.show())
        .and_then(|_| window.set_focus())
        .map_err(|e| AppError::internal(format!("Margin could not show the setup window: {e}")))
}

/// Mark setup done, write the welcome note the first time, close setup and show Margin on that
/// note. Runs for Open Margin and for closing the window early; a second call while one is
/// running does nothing. Margin always ends up with its main window showing, even on an error.
pub async fn finish(app: &AppHandle) -> AppResult<()> {
    let shell = app.state::<Shell>();
    if shell.onboarding_finishing.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = mark_done(&shell).await;
    if let Some(window) = app.get_webview_window(LABEL)
        && let Err(error) = window.destroy()
    {
        tracing::warn!(%error, "could not close the setup window");
    }
    super::show_main(app);
    shell.ui_ready();
    shell.onboarding_finishing.store(false, Ordering::SeqCst);
    result
}

async fn mark_done(shell: &Shell) -> AppResult<()> {
    let mut settings = shell.db.get_settings().await?;
    if settings.onboarding_done {
        return Ok(());
    }
    settings.onboarding_done = true;
    shell.db.set_settings(&settings).await?;
    let saved = shell.db.get_settings().await?;
    shell.events.emit(AppEvent::SettingsChanged { settings: saved });
    let style = if cfg!(target_os = "macos") { ShortcutStyle::Mac } else { ShortcutStyle::Other };
    // A missing welcome note is no reason to keep someone out of the app.
    match shell.db.create_welcome_note(style).await {
        Ok(note) => {
            shell.events.emit(AppEvent::DataChanged { scopes: vec![DataScope::Notes], note_id: None });
            shell.events.emit(AppEvent::OpenNote { note_id: note.id });
        }
        Err(error) => tracing::warn!(error = %AppError::from(error).message, "could not write the welcome note"),
    }
    Ok(())
}
