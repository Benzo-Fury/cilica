//! The tray (menu bar) icon and its menu. Idle: Open Cilica · Start recording · Listening for
//! meetings · Quit Cilica. While recording, the menu bar shows the elapsed time next to the glyph
//! and the menu leads with what is happening and what you can do about it (RecordingStatus doc):
//! the status line · Keep recording (only while a detected meeting is ending) · Pause or Resume ·
//! Stop and write notes · Open note.

use cilica_domain::{RecordingTrigger, Settings};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};

use super::Shell;
use super::status::{Mode, Status};

const OPEN: &str = "cilica.tray.open";
const OPEN_NOTE: &str = "cilica.tray.open-note";
const RECORD: &str = "cilica.tray.record";
const STOP: &str = "cilica.tray.stop";
const KEEP: &str = "cilica.tray.keep";
const PAUSE: &str = "cilica.tray.pause";
const STATUS: &str = "cilica.tray.status";
const LISTEN: &str = "cilica.tray.listen";
const QUIT: &str = "cilica.tray.quit";

/// What the menu was built for. A new shape rebuilds the menu; anything else updates it in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
    mode: Mode,
    has_note: bool,
    paused: bool,
}

impl Shape {
    fn of(status: &Status) -> Self {
        Self { mode: status.mode, has_note: status.has_note, paused: status.paused }
    }
}

/// Handles kept to update the menu and icon.
pub struct Items {
    tray: TrayIcon,
    listen: CheckMenuItem<Wry>,
    /// The disabled status line, while not idle.
    headline: Option<MenuItem<Wry>>,
    shape: Shape,
}

/// The menu for one shape, and the items that change while it is up.
struct Built {
    menu: Menu<Wry>,
    listen: CheckMenuItem<Wry>,
    headline: Option<MenuItem<Wry>>,
}

pub fn create(app: &AppHandle, settings: &Settings) -> tauri::Result<()> {
    let idle = Status::idle();
    let shape = Shape::of(&idle);
    let built = build_menu(app, shape, &idle, settings.recording.auto_detect)?;
    let tray = TrayIconBuilder::with_id("cilica")
        .icon(icon(app, false))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(&idle.tooltip)
        .menu(&built.menu)
        .show_menu_on_left_click(true)
        .build(app)?;
    if let Ok(mut items) = app.state::<Shell>().tray.lock() {
        *items = Some(Items { tray, listen: built.listen, headline: built.headline, shape });
    }
    Ok(())
}

fn build_menu(app: &AppHandle, shape: Shape, status: &Status, listening: bool) -> tauri::Result<Built> {
    let listen = CheckMenuItem::with_id(app, LISTEN, "Listening for meetings", true, listening, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit Cilica", true, None::<&str>)?;
    let separator = || PredefinedMenuItem::separator(app);
    let headline = match &status.headline {
        Some(text) if shape.mode != Mode::Idle => Some(MenuItem::with_id(app, STATUS, text, false, None::<&str>)?),
        _ => None,
    };
    // Open note when there is one to open, otherwise Open Cilica.
    let open = if shape.has_note {
        MenuItem::with_id(app, OPEN_NOTE, "Open note", true, None::<&str>)?
    } else {
        MenuItem::with_id(app, OPEN, "Open Cilica", true, None::<&str>)?
    };
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
    let menu = Menu::new(app)?;
    if let Some(headline) = &headline {
        menu.append(headline)?;
        menu.append(&separator()?)?;
    }
    match shape.mode {
        Mode::Idle => {
            menu.append(&open)?;
            menu.append(&item(RECORD, "Start recording")?)?;
        }
        Mode::Recording | Mode::Ending => {
            if shape.mode == Mode::Ending {
                menu.append(&item(KEEP, "Keep recording")?)?;
            }
            menu.append(&item(PAUSE, if shape.paused { "Resume" } else { "Pause" })?)?;
            menu.append(&item(STOP, "Stop and write notes")?)?;
            menu.append(&separator()?)?;
            menu.append(&open)?;
        }
        Mode::Finishing => menu.append(&open)?,
    }
    menu.append(&separator()?)?;
    menu.append(&listen)?;
    menu.append(&separator()?)?;
    menu.append(&quit)?;
    Ok(Built { menu, listen, headline })
}

/// Handle a click on one of the tray's items. Returns false for other menu ids.
pub fn on_menu_event(app: &AppHandle, id: &str) -> bool {
    match id {
        OPEN => super::show_main(app),
        OPEN_NOTE => super::open_recording_note(app),
        RECORD => super::toggle_recording(app, RecordingTrigger::Tray),
        STOP => super::stop_recording(app),
        KEEP => super::keep_recording(app),
        PAUSE => super::toggle_pause(app),
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

/// Bring the icon, title, tooltip and menu in line with `status`. `prev` is what they show now.
pub fn refresh(app: &AppHandle, status: &Status, prev: &Status) {
    let shell = app.state::<Shell>();
    let shape = Shape::of(status);
    // Take what is needed and let go of the lock: building a menu hops to the main thread, which
    // may be waiting on this lock in a menu click.
    let Some((tray, old_shape, headline)) =
        shell.tray.lock().ok().and_then(|items| items.as_ref().map(|i| (i.tray.clone(), i.shape, i.headline.clone())))
    else {
        return;
    };
    if shape != old_shape {
        match build_menu(app, shape, status, shell.settings().recording.auto_detect) {
            Ok(built) => {
                let _ = tray.set_menu(Some(built.menu.clone()));
                if let Ok(mut items) = shell.tray.lock()
                    && let Some(items) = items.as_mut()
                {
                    items.listen = built.listen;
                    items.headline = built.headline;
                    items.shape = shape;
                }
            }
            Err(error) => tracing::warn!(%error, "could not rebuild the tray menu"),
        }
    } else if status.headline != prev.headline
        && let (Some(item), Some(text)) = (headline, &status.headline)
    {
        let _ = item.set_text(text);
    }
    if status.capturing() != prev.capturing() || shape != old_shape {
        let _ = tray.set_icon(Some(icon(app, status.capturing())));
        let _ = tray.set_icon_as_template(cfg!(target_os = "macos"));
    }
    if status.tray_title != prev.tray_title {
        let _ = tray.set_title(status.tray_title.as_deref());
    }
    if status.tooltip != prev.tooltip {
        let _ = tray.set_tooltip(Some(&status.tooltip));
    }
}

pub fn set_listening(app: &AppHandle, on: bool) {
    let shell = app.state::<Shell>();
    if let Ok(items) = shell.tray.lock()
        && let Some(items) = items.as_ref()
    {
        let _ = items.listen.set_checked(on);
    }
}

/// On macOS a template glyph drawn here (the ridgeline mark, with a larger sun while recording);
/// elsewhere the app icon, with a red dot in its corner while recording.
fn icon(app: &AppHandle, recording: bool) -> Image<'static> {
    if !cfg!(target_os = "macos")
        && let Some(icon) = app.default_window_icon()
    {
        let (width, height) = (icon.width(), icon.height());
        let mut rgba = icon.rgba().to_vec();
        if recording {
            super::taskbar::stamp_rec_dot(&mut rgba, width, height);
        }
        return Image::new_owned(rgba, width, height);
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
