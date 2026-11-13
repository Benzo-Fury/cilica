//! The tray (menu bar) icon and its menu:
//! Open Cilica · Start/Stop recording · Listening for meetings · Quit Cilica.

use cilica_domain::{RecordingPhase, RecordingTrigger, Settings};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};

use super::Shell;

const OPEN: &str = "cilica.tray.open";
const RECORD: &str = "cilica.tray.record";
const LISTEN: &str = "cilica.tray.listen";
const QUIT: &str = "cilica.tray.quit";

/// Handles kept to update the menu and icon.
pub struct Items {
    tray: TrayIcon,
    record: MenuItem<Wry>,
    listen: CheckMenuItem<Wry>,
}

pub fn create(app: &AppHandle, settings: &Settings) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, OPEN, "Open Cilica", true, None::<&str>)?;
    let record = MenuItem::with_id(app, RECORD, "Start recording", true, None::<&str>)?;
    let listen = CheckMenuItem::with_id(app, LISTEN, "Listening for meetings", true, settings.recording.auto_detect, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit Cilica", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &record, &PredefinedMenuItem::separator(app)?, &listen, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    let tray = TrayIconBuilder::with_id("cilica")
        .icon(icon(app, false))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("Cilica")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .build(app)?;
    if let Ok(mut items) = app.state::<Shell>().tray.lock() {
        *items = Some(Items { tray, record, listen });
    }
    Ok(())
}

/// Handle a click on one of the tray's items. Returns false for other menu ids.
pub fn on_menu_event(app: &AppHandle, id: &str) -> bool {
    match id {
        OPEN => super::show_main(app),
        RECORD => super::toggle_recording(app, RecordingTrigger::Tray),
        LISTEN => {
            let on = !app.state::<Shell>().settings().recording.auto_detect;
            // Reflect the click at once; the SettingsChanged round trip confirms it.
            set_listening(app, on);
            super::save_settings(app, move |settings| settings.recording.auto_detect = on);
        }
        QUIT => super::request_quit(app),
        _ => return false,
    }
    true
}

/// Follow the recording phase: the label says what a click will do.
pub fn set_recording(app: &AppHandle, phase: RecordingPhase) {
    let shell = app.state::<Shell>();
    let Ok(items) = shell.tray.lock() else { return };
    let Some(items) = items.as_ref() else { return };
    let (label, enabled, live) = match phase {
        RecordingPhase::Starting | RecordingPhase::Recording => ("Stop recording", true, true),
        RecordingPhase::Stopping => ("Stopping…", false, true),
        RecordingPhase::Idle | RecordingPhase::Summarizing => ("Start recording", true, false),
    };
    let _ = items.record.set_text(label);
    let _ = items.record.set_enabled(enabled);
    let _ = items.tray.set_icon(Some(icon(app, live)));
    let _ = items.tray.set_icon_as_template(cfg!(target_os = "macos"));
    let _ = items.tray.set_tooltip(Some(if live { "Cilica · Recording" } else { "Cilica" }));
}

pub fn set_listening(app: &AppHandle, on: bool) {
    let shell = app.state::<Shell>();
    if let Ok(items) = shell.tray.lock()
        && let Some(items) = items.as_ref()
    {
        let _ = items.listen.set_checked(on);
    }
}

/// On macOS a template glyph drawn here (the ridgeline mark, with a larger sun while recording); elsewhere the app icon.
fn icon(app: &AppHandle, recording: bool) -> Image<'static> {
    if !cfg!(target_os = "macos")
        && let Some(icon) = app.default_window_icon()
    {
        return icon.clone().to_owned();
    }
    let (rgba, size) = draw_glyph(recording);
    Image::new_owned(rgba, size, size)
}

/// A 36 px (18 pt @2x) black-on-transparent glyph matching the app icon: three ridgelines drawn
/// as strokes, under a small sun that grows while recording. Anti-aliased by 4×4 supersampling.
fn draw_glyph(recording: bool) -> (Vec<u8>, u32) {
    const SIZE: u32 = 36;
    const SS: u32 = 4;
    let stroke = 2.6_f32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut covered = 0u32;
            for sy in 0..SS {
                for sx in 0..SS {
                    let px = x as f32 + (sx as f32 + 0.5) / SS as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SS as f32;
                    if glyph_hit(px, py, stroke, recording) {
                        covered += 1;
                    }
                }
            }
            let alpha = (covered * 255 / (SS * SS)) as u8;
            let i = ((y * SIZE + x) * 4) as usize;
            rgba[i + 3] = alpha;
        }
    }
    (rgba, SIZE)
}

/// Ridgelines as (baseline y, amplitude, phase): far ridge on top, gentler as they come closer.
const RIDGES: [(f32, f32, f32); 3] = [(15.0, 2.4, 0.4), (22.0, 2.0, 1.5), (29.0, 1.6, 2.6)];

fn glyph_hit(x: f32, y: f32, stroke: f32, recording: bool) -> bool {
    // The sun above the far ridge: small at rest, large while recording.
    let radius = if recording { 4.6_f32 } else { 2.6 };
    if ((x - 26.0).powi(2) + (y - 6.5).powi(2)).sqrt() <= radius {
        return true;
    }
    // Each ridgeline from x 4 to 32 with round caps: the nearest of a dense run of points on it.
    RIDGES.iter().any(|&(base, amp, phase)| {
        (0..=112).any(|i| {
            let cx = 4.0 + i as f32 * 0.25;
            let cy = base - amp * (cx / 5.5 + phase).sin();
            ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() <= stroke / 2.0
        })
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn glyph_has_ink_and_space() {
        for recording in [false, true] {
            let (rgba, size) = super::draw_glyph(recording);
            assert_eq!(rgba.len(), (size * size * 4) as usize);
            let inked = rgba.chunks(4).filter(|p| p[3] > 128).count();
            assert!(inked > 100 && inked < (size * size / 2) as usize, "inked {inked}");
        }
    }
}
