//! The Dynamic Island: a small always-on-top window showing recording state while Cilica is in the
//! background (docs/design-system/components/DynamicIsland). It loads the same frontend with
//! `?window=island` (its label is also `island`), is created hidden at startup, and shows by
//! itself while capturing with the main window unfocused.

use std::sync::atomic::Ordering;

use tauri::{AppHandle, LogicalSize, Manager, Monitor, PhysicalPosition, Theme, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::Shell;

pub const LABEL: &str = "island";
/// Compact size in points.
pub const COMPACT: (f64, f64) = (168.0, 32.0);
/// Gap below the menu bar or top edge when there is no notch, in points.
const TOP_GAP: f64 = 8.0;

pub fn create(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?window=island".into()))
        .title("Cilica recording")
        .inner_size(COMPACT.0, COMPACT.1)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .visible(false)
        .theme(Some(Theme::Dark));
    #[cfg(target_os = "macos")]
    let builder = builder.accept_first_mouse(true);
    let window = builder.build()?;
    #[cfg(target_os = "macos")]
    macos::configure(&window);
    Ok(window)
}

/// Show while capturing and the main window is not focused; hide otherwise.
pub fn sync(app: &AppHandle) {
    let Some(shell) = app.try_state::<Shell>() else { return };
    if super::is_capturing(app) && !shell.main_focused() {
        if !shell.island_visible.load(Ordering::Relaxed) {
            show(app);
        }
    } else if shell.island_visible.load(Ordering::Relaxed) {
        hide(app);
    }
}

/// Place the island top-centre of the active display and show it without taking focus.
pub fn show(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = handle.get_webview_window(LABEL) else { return };
        place(&handle, &window);
        if let Err(error) = window.show() {
            tracing::warn!(%error, "could not show the island");
            return;
        }
        handle.state::<Shell>().island_visible.store(true, Ordering::Relaxed);
    });
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
    if let Some(shell) = app.try_state::<Shell>() {
        shell.island_visible.store(false, Ordering::Relaxed);
    }
}

/// Resize (points) and keep it centred under the menu bar.
pub fn set_size(app: &AppHandle, width: f64, height: f64) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(LABEL) else { return Ok(()) };
    window.set_size(LogicalSize::new(width, height))?;
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(LABEL) {
            place(&handle, &window);
        }
    })
}

/// The display the island belongs on: the one under the pointer, else the primary one.
fn active_monitor(app: &AppHandle) -> Option<Monitor> {
    app.cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())
}

/// Whether the active display has a camera notch (macOS). Must run on the main thread.
pub fn active_display_has_notch(app: &AppHandle) -> bool {
    active_monitor(app).is_some_and(|m| has_notch(&m))
}

fn place(app: &AppHandle, window: &WebviewWindow) {
    let Some(monitor) = active_monitor(app) else { return };
    let Ok(size) = window.outer_size() else { return };
    let area = monitor.work_area();
    let gap = if has_notch(&monitor) { 0.0 } else { TOP_GAP * monitor.scale_factor() };
    let x = area.position.x + (area.size.width as i32 - size.width as i32) / 2;
    let y = area.position.y + gap.round() as i32;
    if let Err(error) = window.set_position(PhysicalPosition::new(x, y)) {
        tracing::warn!(%error, "could not place the island");
    }
}

#[cfg(target_os = "macos")]
fn has_notch(monitor: &Monitor) -> bool {
    macos::has_notch(monitor.size().width, monitor.size().height)
}

#[cfg(not(target_os = "macos"))]
fn has_notch(_monitor: &Monitor) -> bool {
    false
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSScreen, NSWindow, NSWindowCollectionBehavior};
    use tauri::WebviewWindow;

    /// On every Space and over full-screen apps, out of Cmd+` cycling and Mission Control.
    pub fn configure(window: &WebviewWindow) {
        let Ok(ptr) = window.ns_window() else { return };
        if ptr.is_null() || MainThreadMarker::new().is_none() {
            return;
        }
        // SAFETY: `ns_window` is the live NSWindow of this webview window and we are on the main thread.
        let ns_window: &NSWindow = unsafe { &*ptr.cast::<NSWindow>() };
        ns_window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        ns_window.setHidesOnDeactivate(false);
    }

    /// True when a screen with this pixel size has a top safe-area inset (the notch). Matching by
    /// size is enough: only built-in displays have notches.
    pub fn has_notch(width: u32, height: u32) -> bool {
        let Some(mtm) = MainThreadMarker::new() else { return false };
        NSScreen::screens(mtm).iter().any(|screen| {
            let scale = screen.backingScaleFactor();
            let frame = screen.frame();
            screen.safeAreaInsets().top > 0.0
                && (frame.size.width * scale).round() as u32 == width
                && (frame.size.height * scale).round() as u32 == height
        })
    }
}
