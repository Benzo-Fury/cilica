//! Trackpad haptics (docs/design-system/sound-and-haptics.md). macOS only; a no-op elsewhere.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

/// The three NSHapticFeedbackManager patterns Cilica uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum HapticKind {
    /// A dragged item snaps into place; a resize snaps to its default width.
    Alignment,
    /// A permission was granted.
    Generic,
    /// Slider detents.
    LevelChange,
}

#[cfg(target_os = "macos")]
pub fn perform(app: &AppHandle, kind: HapticKind) {
    use objc2_app_kit::{NSHapticFeedbackManager, NSHapticFeedbackPattern, NSHapticFeedbackPerformanceTime, NSHapticFeedbackPerformer};
    let _ = app.run_on_main_thread(move || {
        let pattern = match kind {
            HapticKind::Alignment => NSHapticFeedbackPattern::Alignment,
            HapticKind::Generic => NSHapticFeedbackPattern::Generic,
            HapticKind::LevelChange => NSHapticFeedbackPattern::LevelChange,
        };
        NSHapticFeedbackManager::defaultPerformer().performFeedbackPattern_performanceTime(pattern, NSHapticFeedbackPerformanceTime::Now);
    });
}

#[cfg(not(target_os = "macos"))]
pub fn perform(_app: &AppHandle, _kind: HapticKind) {}
