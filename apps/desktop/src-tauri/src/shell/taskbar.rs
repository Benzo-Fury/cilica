//! The main window's taskbar button (Windows, Linux) and Dock icon (macOS) while recording
//! (RecordingStatus doc):
//! - macOS: a Dock badge with the elapsed time.
//! - Windows: a red dot over the taskbar button while capturing (not while paused), the status in the window title (shown on hover)
//!   and Pause, Stop, Open note and Keep recording under the thumbnail (see [`super::thumbbar`]).
//! - Everywhere: an indeterminate progress bar while the notes are written, and one bounce or
//!   flash when a detected meeting ends and recording is about to stop on its own.

use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::{AppHandle, Manager, UserAttentionType};

use super::MAIN;
use super::status::{Mode, Status};

/// The `rec` colour (tokens.json, light theme): recording and nothing else.
const REC: [u8; 3] = [0xe2, 0x35, 0x32];

/// Bring the taskbar button or Dock icon in line with `status`. `prev` is what it shows now.
pub fn refresh(app: &AppHandle, status: &Status, prev: &Status) {
    let Some(window) = app.get_webview_window(MAIN) else { return };

    #[cfg(target_os = "macos")]
    if status.badge != prev.badge {
        let _ = window.set_badge_label(status.badge.clone());
    }

    #[cfg(target_os = "windows")]
    if status.capturing() != prev.capturing() {
        let overlay = status.capturing().then(|| tauri::image::Image::new_owned(rec_dot(32), 32, 32));
        if let Err(error) = window.set_overlay_icon(overlay) {
            tracing::warn!(%error, "could not set the taskbar overlay");
        }
    }

    #[cfg(not(target_os = "macos"))]
    if status.window_title != prev.window_title {
        let _ = window.set_title(&status.window_title);
    }

    if (status.mode == Mode::Finishing) != (prev.mode == Mode::Finishing) {
        let progress = if status.mode == Mode::Finishing { ProgressBarStatus::Indeterminate } else { ProgressBarStatus::None };
        let _ = window.set_progress_bar(ProgressBarState { status: Some(progress), progress: None });
    }

    if (status.mode == Mode::Ending) != (prev.mode == Mode::Ending) {
        let attention = (status.mode == Mode::Ending).then_some(UserAttentionType::Informational);
        let _ = window.request_user_attention(attention);
    }

    #[cfg(target_os = "windows")]
    super::thumbbar::refresh(&window, status);
}

/// A `size` × `size` red dot with a thin white ring, anti-aliased, as RGBA.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn rec_dot(size: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    stamp(&mut rgba, size, size, size as f32 / 2.0, size as f32 / 2.0, size as f32 / 2.0);
    rgba
}

/// Paint the red dot over the bottom-right corner of an RGBA icon.
pub fn stamp_rec_dot(rgba: &mut [u8], width: u32, height: u32) {
    let radius = width.min(height) as f32 * 0.24;
    stamp(rgba, width, height, width as f32 - radius, height as f32 - radius, radius);
}

/// A dot of `radius` centred at (`cx`, `cy`): red inside, a white ring at its edge, blended over
/// what is there. 4 × 4 supersampled.
fn stamp(rgba: &mut [u8], width: u32, height: u32, cx: f32, cy: f32, radius: f32) {
    const SS: u32 = 4;
    let ring = (radius * 0.16).max(1.0);
    for y in 0..height {
        for x in 0..width {
            let (mut dot, mut red) = (0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let px = x as f32 + (sx as f32 + 0.5) / SS as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SS as f32;
                    let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                    if d <= radius {
                        dot += 1;
                        if d <= radius - ring {
                            red += 1;
                        }
                    }
                }
            }
            if dot == 0 {
                continue;
            }
            let n = (SS * SS) as f32;
            let coverage = dot as f32 / n;
            let share_red = red as f32 / dot as f32;
            let i = ((y * width + x) * 4) as usize;
            for c in 0..3 {
                let src = REC[c] as f32 * share_red + 255.0 * (1.0 - share_red);
                rgba[i + c] = (src * coverage + rgba[i + c] as f32 * (1.0 - coverage)).round() as u8;
            }
            let alpha = rgba[i + 3] as f32 / 255.0;
            rgba[i + 3] = ((coverage + alpha * (1.0 - coverage)) * 255.0).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_is_red_in_the_middle_and_clear_in_the_corners() {
        let rgba = rec_dot(32);
        let px = |x: u32, y: u32| &rgba[((y * 32 + x) * 4) as usize..][..4];
        assert_eq!(px(16, 16), &[REC[0], REC[1], REC[2], 255]);
        assert_eq!(px(0, 0)[3], 0);
    }

    #[test]
    fn stamp_marks_only_the_corner() {
        let mut rgba = vec![10u8; 64 * 64 * 4];
        stamp_rec_dot(&mut rgba, 64, 64);
        assert_eq!(&rgba[..4], &[10, 10, 10, 10]);
        let corner = ((56 * 64 + 56) * 4) as usize;
        assert_eq!(&rgba[corner..corner + 3], &REC);
    }
}
