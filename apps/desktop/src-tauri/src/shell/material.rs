//! The native window material behind the sidebar and assistant (docs/design-system/materials.md).
//!
//! | OS            | Material                                   |
//! | ---           | ---                                        |
//! | macOS 26+     | Liquid Glass (`NSGlassEffectView`)         |
//! | macOS 14–15   | `NSVisualEffectView`, Sidebar material     |
//! | Windows 11    | Mica, following the light/dark theme       |
//! | Windows 10    | Acrylic                                    |
//! | Linux         | none: the UI paints `bg-window`            |
//!
//! Cleared when Settings › Appearance › Transparency is off or the OS asks to reduce transparency.
//!
//! The current shell is one opaque sheet (docs/design-system/materials.md), so no material is
//! applied: [`WINDOW_MATERIAL`] is off. The table above is what turning it back on gives.

use tauri::{AppHandle, Manager, Theme, WebviewWindow};

use super::{MAIN, Shell};

/// Whether the main window gets a native material at all. Off: the shell paints opaque grounds.
const WINDOW_MATERIAL: bool = false;

/// Which material is on the main window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Material {
    LiquidGlass,
    Vibrancy,
    Mica,
    Acrylic,
}

/// What was asked for last time and what was actually applied.
#[derive(Debug, Default)]
pub struct State {
    /// `(enabled, dark)` of the last sync; `None` before the first one.
    wanted: Option<(bool, bool)>,
    applied: Option<Material>,
}

/// True when a native material is on the main window right now.
pub fn has_material(app: &AppHandle) -> bool {
    app.try_state::<Shell>().and_then(|s| s.material.lock().ok().map(|m| m.applied.is_some())).unwrap_or(false)
}

/// Recompute the material from settings, theme and the OS accessibility setting, and reapply it
/// on the main thread if anything changed.
pub fn sync(app: &AppHandle) {
    let handle = app.clone();
    if let Err(error) = app.run_on_main_thread(move || sync_on_main_thread(&handle)) {
        tracing::warn!(%error, "could not schedule the window material update");
    }
}

fn sync_on_main_thread(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN) else { return };
    let shell = app.state::<Shell>();
    let appearance = shell.settings().appearance;
    let enabled = WINDOW_MATERIAL && appearance.transparency && !os_reduces_transparency();
    // Only Mica takes the theme as a parameter; the macOS materials follow the window appearance.
    let dark = cfg!(target_os = "windows") && is_dark(&window, appearance.theme);
    let Ok(mut state) = shell.material.lock() else { return };
    if state.wanted == Some((enabled, dark)) {
        return;
    }
    clear(&window);
    state.applied = if enabled { apply(&window, dark) } else { None };
    state.wanted = Some((enabled, dark));
    tracing::info!(material = ?state.applied, enabled, "window material");
}

fn is_dark(window: &WebviewWindow, mode: cilica_domain::ThemeMode) -> bool {
    match mode {
        cilica_domain::ThemeMode::Light => false,
        cilica_domain::ThemeMode::Dark => true,
        cilica_domain::ThemeMode::System => matches!(window.theme(), Ok(Theme::Dark)),
    }
}

#[cfg(target_os = "macos")]
fn apply(window: &WebviewWindow, _dark: bool) -> Option<Material> {
    use window_vibrancy::{LiquidGlassOptions, NSVisualEffectMaterial, NSVisualEffectState};
    match window_vibrancy::apply_liquid_glass(window, LiquidGlassOptions::default()) {
        Ok(()) => return Some(Material::LiquidGlass),
        Err(error) => tracing::debug!(%error, "Liquid Glass unavailable, using vibrancy"),
    }
    match window_vibrancy::apply_vibrancy(window, NSVisualEffectMaterial::Sidebar, Some(NSVisualEffectState::FollowsWindowActiveState), None) {
        Ok(()) => Some(Material::Vibrancy),
        Err(error) => {
            tracing::warn!(%error, "could not apply window vibrancy");
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn clear(window: &WebviewWindow) {
    let _ = window_vibrancy::clear_liquid_glass(window);
    let _ = window_vibrancy::clear_vibrancy(window);
}

#[cfg(target_os = "macos")]
fn os_reduces_transparency() -> bool {
    objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency()
}

#[cfg(target_os = "windows")]
fn apply(window: &WebviewWindow, dark: bool) -> Option<Material> {
    match window_vibrancy::apply_mica(window, Some(dark)) {
        Ok(()) => return Some(Material::Mica),
        Err(error) => tracing::debug!(%error, "Mica unavailable, using Acrylic"),
    }
    let tint = if dark { (32, 32, 32, 160) } else { (243, 243, 243, 160) };
    match window_vibrancy::apply_acrylic(window, Some(tint)) {
        Ok(()) => Some(Material::Acrylic),
        Err(error) => {
            tracing::warn!(%error, "could not apply Acrylic");
            None
        }
    }
}

#[cfg(target_os = "windows")]
fn clear(window: &WebviewWindow) {
    let _ = window_vibrancy::clear_mica(window);
    let _ = window_vibrancy::clear_acrylic(window);
}

/// Settings › Personalisation › Colours › Transparency effects.
#[cfg(target_os = "windows")]
fn os_reduces_transparency() -> bool {
    windows_registry::CURRENT_USER
        .open(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|key| key.get_u32("EnableTransparency"))
        .is_ok_and(|enabled| enabled == 0)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn apply(_window: &WebviewWindow, _dark: bool) -> Option<Material> {
    None
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn clear(_window: &WebviewWindow) {}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn os_reduces_transparency() -> bool {
    false
}
