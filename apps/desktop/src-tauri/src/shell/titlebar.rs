//! Double-click on the title band.
//!
//! The webview covers the native title bar (overlay title bar), so AppKit never sees the
//! double-click. Tauri's drag-region script would answer it with its own maximise, which skips the
//! user's "Double-click a window's title bar to" choice and on macOS does not run the system's
//! window animation. Instead the UI calls `title_bar_double_click` and this module performs the
//! same action AppKit would, with the system's animation.

use tauri::WebviewWindow;

/// What the system setting asks a title-bar double-click to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Fill the screen (window tiling, macOS 15 and later).
    Fill,
    /// Toggle between the standard frame and the user's frame ("Zoom", stored as "Maximize").
    Zoom,
    Minimize,
    Nothing,
}

impl Action {
    /// Read `AppleActionOnDoubleClick` ("Fill", "Maximize", "Minimize", "None"), falling back to
    /// the pre-10.11 `AppleMiniaturizeOnDoubleClick` flag, then to zoom.
    pub fn from_defaults(action: Option<&str>, legacy_minimize: bool) -> Self {
        match action {
            Some("Fill") => Action::Fill,
            Some("Maximize" | "Zoom") => Action::Zoom,
            Some("Minimize") => Action::Minimize,
            Some("None") => Action::Nothing,
            _ if legacy_minimize => Action::Minimize,
            _ => Action::Zoom,
        }
    }
}

/// Do what a native title-bar double-click does to `window`.
#[cfg(target_os = "macos")]
pub fn double_click(window: &WebviewWindow) -> tauri::Result<()> {
    let target = window.clone();
    window.run_on_main_thread(move || macos::perform(&target))
}

/// Elsewhere the drag-region double-click toggles maximise, as the platform does.
#[cfg(not(target_os = "macos"))]
pub fn double_click(window: &WebviewWindow) -> tauri::Result<()> {
    if window.is_maximized()? { window.unmaximize() } else { window.maximize() }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::runtime::{AnyObject, Sel};
    use objc2::{MainThreadMarker, msg_send, sel};
    use std::sync::Mutex;
    use std::time::Duration;

    use objc2_app_kit::NSWindow;
    use objc2_foundation::{NSRect, NSUserDefaults, ns_string};
    use tauri::WebviewWindow;

    use super::Action;

    /// The main window's frame before it last filled the screen.
    static BEFORE_FILL: Mutex<Option<NSRect>> = Mutex::new(None);

    /// Longer than the system's untile animation.
    const UNTILE_SETTLE: Duration = Duration::from_millis(1000);

    /// Read the system setting and act on the window. Must run on the main thread.
    pub fn perform(window: &WebviewWindow) {
        let Ok(ptr) = window.ns_window() else { return };
        if ptr.is_null() || MainThreadMarker::new().is_none() {
            return;
        }
        // SAFETY: `ns_window` is the live NSWindow of this webview window and we are on the main thread.
        let ns_window: &NSWindow = unsafe { &*ptr.cast::<NSWindow>() };

        // Read the user's choice from the global domain, as AppKit does.
        let defaults = NSUserDefaults::standardUserDefaults();
        let action = defaults.stringForKey(ns_string!("AppleActionOnDoubleClick")).map(|s| s.to_string());
        let legacy_minimize = defaults.boolForKey(ns_string!("AppleMiniaturizeOnDoubleClick"));

        match Action::from_defaults(action.as_deref(), legacy_minimize) {
            Action::Nothing => {}
            Action::Minimize => ns_window.performMiniaturize(None),
            // Cilica has no custom standard frame, so Zoom and Fill reach the same size. Both go
            // through window tiling: plain `zoom:` animates the frame, but the webview does not
            // repaint until it ends, so on screen the window jumps.
            Action::Zoom | Action::Fill => toggle_fill(window, ns_window),
        }
    }

    /// Fill the screen, or put a filled window back where it was, through the system's tiling
    /// actions (Window > Fill), which the window server animates. Systems without tiling
    /// (before macOS 15) zoom instead.
    fn toggle_fill(window: &WebviewWindow, ns_window: &NSWindow) {
        let frame = ns_window.frame();
        let filled = ns_window.screen().is_some_and(|screen| screen.visibleFrame() == frame);
        let action: Sel = if filled { sel!(_zoomUntile:) } else { sel!(_zoomFill:) };
        // SAFETY: `respondsToSelector:` is always safe to send.
        let supported: bool = unsafe { msg_send![ns_window, respondsToSelector: action] };
        if !supported {
            ns_window.performZoom(None);
            return;
        }

        if !filled && let Ok(mut before) = BEFORE_FILL.lock() {
            *before = Some(frame);
        }
        let sender: *const AnyObject = std::ptr::null();
        // SAFETY: the window answers `action`, a menu action that takes a sender (nil) and
        // returns nothing (so `performSelector:` hands back nil); we are on the main thread.
        let _: *mut AnyObject = unsafe { msg_send![ns_window, performSelector: action, withObject: sender] };
        if filled {
            settle_after_untile(window);
        }
    }

    /// Untile lands a point or two off the frame it left, so repeated fills would creep. Once the
    /// animation is over, put the window exactly back (only if it is still that close: the user
    /// may have moved it meanwhile).
    fn settle_after_untile(window: &WebviewWindow) {
        let Some(before) = BEFORE_FILL.lock().ok().and_then(|mut b| b.take()) else { return };
        let window = window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(UNTILE_SETTLE);
            let target = window.clone();
            let _ = window.run_on_main_thread(move || {
                let Ok(ptr) = target.ns_window() else { return };
                if ptr.is_null() || MainThreadMarker::new().is_none() {
                    return;
                }
                // SAFETY: as in `perform`: the live NSWindow, on the main thread.
                let ns_window: &NSWindow = unsafe { &*ptr.cast::<NSWindow>() };
                let now = ns_window.frame();
                if now != before && super::nearly_equal(rect(now), rect(before)) {
                    ns_window.setFrame_display(before, true);
                }
            });
        });
    }

    fn rect(r: NSRect) -> [f64; 4] {
        [r.origin.x, r.origin.y, r.size.width, r.size.height]
    }
}

/// Two frames (x, y, width, height) within a few points of each other.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn nearly_equal(a: [f64; 4], b: [f64; 4]) -> bool {
    a.iter().zip(b).all(|(a, b)| (a - b).abs() <= 4.0)
}

#[cfg(test)]
mod tests {
    use super::{Action, nearly_equal};

    #[test]
    fn snaps_back_only_when_untile_landed_close() {
        assert!(nearly_equal([195.0, 110.0, 1200.0, 760.0], [193.0, 110.0, 1202.0, 762.0]));
        assert!(!nearly_equal([195.0, 110.0, 1200.0, 760.0], [400.0, 110.0, 1200.0, 760.0]));
    }

    #[test]
    fn reads_the_system_setting() {
        assert_eq!(Action::from_defaults(Some("Fill"), false), Action::Fill);
        assert_eq!(Action::from_defaults(Some("Maximize"), false), Action::Zoom);
        assert_eq!(Action::from_defaults(Some("Minimize"), false), Action::Minimize);
        assert_eq!(Action::from_defaults(Some("None"), true), Action::Nothing);
    }

    #[test]
    fn falls_back_to_the_legacy_flag_then_zoom() {
        assert_eq!(Action::from_defaults(None, true), Action::Minimize);
        assert_eq!(Action::from_defaults(None, false), Action::Zoom);
        assert_eq!(Action::from_defaults(Some("Unknown"), false), Action::Zoom);
    }
}
