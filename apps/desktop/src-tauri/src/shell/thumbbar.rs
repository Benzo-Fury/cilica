//! Windows only: buttons under the main window's taskbar thumbnail while recording (RecordingStatus
//! doc): Pause or Resume, Stop and write notes, Open note, and Keep recording while a detected
//! meeting is ending.
//!
//! The taskbar takes its buttons once, each time it creates the window's button (it announces
//! that with the registered "TaskbarButtonCreated" message), so all three are added hidden then
//! and shown, hidden or relabelled afterwards. Clicks arrive as `WM_COMMAND` with `THBN_CLICKED`, read by a
//! subclass of the main window. Everything here runs on the main thread, where COM lives.

use std::cell::{Cell, RefCell};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use tauri::{AppHandle, Manager, WebviewWindow};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Shell::{
    DefSubclassProc, ITaskbarList3, SetWindowSubclass, TaskbarList, THB_FLAGS, THB_ICON, THB_TOOLTIP, THBF_DISMISSONCLICK, THBF_ENABLED,
    THBF_HIDDEN, THBN_CLICKED, THUMBBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, GetSystemMetrics, HICON, ICONINFO, RegisterWindowMessageW, SM_CXSMICON, WM_COMMAND,
};
use windows::core::w;

use super::status::{Mode, Status};

/// Button ids, in the order they sit under the thumbnail.
const PAUSE: u32 = 1;
const STOP: u32 = 2;
const OPEN: u32 = 3;
const KEEP: u32 = 4;
const IDS: [u32; 4] = [PAUSE, STOP, OPEN, KEEP];
/// The glyph Pause shows while paused.
const RESUME: u32 = 5;
/// Our subclass id on the main window.
const SUBCLASS_ID: usize = 0xC111_CA00;

static APP: OnceLock<AppHandle> = OnceLock::new();
static BUTTON_CREATED: AtomicU32 = AtomicU32::new(0);

/// The taskbar's buttons for our window, once it has taken them.
struct Bar {
    list: ITaskbarList3,
    hwnd: HWND,
    /// Pause, Stop, Open note, Keep recording, Resume.
    icons: [HICON; 5],
}

/// What the buttons should show.
#[derive(Clone, Copy, Default)]
struct Wanted {
    /// Pause, Stop, Open note, Keep recording.
    shown: [bool; 4],
    paused: bool,
}

thread_local! {
    static BAR: RefCell<Option<Bar>> = const { RefCell::new(None) };
    static WANTED: Cell<Wanted> = const { Cell::new(Wanted { shown: [false; 4], paused: false }) };
}

/// Listen for the taskbar button and its clicks. Call once, on the main thread, before the main
/// window first shows.
pub fn install(window: &WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = HWND(hwnd.0);
    let _ = APP.set(window.app_handle().clone());
    // SAFETY: plain Win32 calls on the main thread with the live HWND of our main window; the
    // subclass procedure only uses thread-local and static state.
    unsafe {
        BUTTON_CREATED.store(RegisterWindowMessageW(w!("TaskbarButtonCreated")), Ordering::Relaxed);
        if !SetWindowSubclass(hwnd, Some(subclass), SUBCLASS_ID, 0).as_bool() {
            tracing::warn!("could not listen for taskbar thumbnail buttons");
        }
    }
}

/// Show the buttons that fit `status`. Safe to call from any thread.
pub fn refresh(window: &WebviewWindow, status: &Status) {
    let live = status.mode.is_live();
    let wanted = Wanted {
        shown: [live, live, status.mode != Mode::Idle && status.has_note, status.mode == Mode::Ending],
        paused: status.paused,
    };
    let _ = window.run_on_main_thread(move || {
        WANTED.set(wanted);
        BAR.with_borrow(|bar| {
            if let Some(bar) = bar {
                bar.update();
            }
        });
    });
}

impl Bar {
    /// The taskbar made a new button for our window: give it our buttons.
    fn create(hwnd: HWND) -> windows::core::Result<Self> {
        // SAFETY: COM is initialised on the main thread by the window library; TaskbarList is the
        // documented class for ITaskbarList3.
        let list: ITaskbarList3 = unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)? };
        unsafe { list.HrInit()? };
        let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 64) as u32;
        let icons = [PAUSE, STOP, OPEN, KEEP, RESUME].map(|id| icon(&glyph(id, size), size));
        let bar = Self { list, hwnd, icons };
        unsafe { bar.list.ThumbBarAddButtons(hwnd, &bar.buttons(Wanted::default()))? };
        Ok(bar)
    }

    fn update(&self) {
        // SAFETY: the list and HWND are live for as long as the bar is kept.
        if let Err(error) = unsafe { self.list.ThumbBarUpdateButtons(self.hwnd, &self.buttons(WANTED.get())) } {
            tracing::warn!(%error, "could not update the taskbar thumbnail buttons");
        }
    }

    fn buttons(&self, wanted: Wanted) -> [THUMBBUTTON; 4] {
        std::array::from_fn(|i| {
            let (icon, tip) = match IDS[i] {
                PAUSE if wanted.paused => (self.icons[4], "Resume"),
                PAUSE => (self.icons[0], "Pause"),
                STOP => (self.icons[1], "Stop and write notes"),
                OPEN => (self.icons[2], "Open note"),
                _ => (self.icons[3], "Keep recording"),
            };
            let mut button = THUMBBUTTON { dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS, iId: IDS[i], hIcon: icon, ..Default::default() };
            for (slot, unit) in button.szTip.iter_mut().zip(tip.encode_utf16()) {
                *slot = unit;
            }
            button.dwFlags = match (wanted.shown[i], IDS[i]) {
                (false, _) => THBF_HIDDEN,
                (true, OPEN) => THBF_ENABLED | THBF_DISMISSONCLICK,
                (true, _) => THBF_ENABLED,
            };
            button
        })
    }
}

unsafe extern "system" fn subclass(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    let created = BUTTON_CREATED.load(Ordering::Relaxed);
    if created != 0 && msg == created {
        match Bar::create(hwnd) {
            Ok(bar) => {
                bar.update();
                BAR.set(Some(bar));
            }
            Err(error) => tracing::warn!(%error, "could not add the taskbar thumbnail buttons"),
        }
    } else if msg == WM_COMMAND
        && (wparam.0 >> 16) as u32 & 0xffff == THBN_CLICKED
        && let Some(app) = APP.get()
    {
        match (wparam.0 & 0xffff) as u32 {
            PAUSE => super::toggle_pause(app),
            STOP => super::stop_recording(app),
            OPEN => super::open_recording_note(app),
            KEEP => super::keep_recording(app),
            _ => {}
        }
        return LRESULT(0);
    }
    // SAFETY: hands every other message on to the window's own procedure.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// A `size` × `size` HICON from straight (not premultiplied) RGBA.
fn icon(rgba: &[u8], size: u32) -> HICON {
    let bgra: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
    let mask = vec![0u8; (size.div_ceil(16) * 2 * size) as usize];
    // SAFETY: both buffers outlive the calls and match the sizes given; the bitmaps are copied
    // into the icon and freed right after.
    unsafe {
        let color = CreateBitmap(size as i32, size as i32, 1, 32, Some(bgra.as_ptr().cast()));
        let mono = CreateBitmap(size as i32, size as i32, 1, 1, Some(mask.as_ptr().cast()));
        let info = ICONINFO { fIcon: true.into(), xHotspot: 0, yHotspot: 0, hbmMask: mono, hbmColor: color };
        let icon = CreateIconIndirect(&info).unwrap_or_default();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mono.into());
        icon
    }
}

/// The glyph for a button, drawn for the dark thumbnail strip: white pause bars and play
/// triangle (Resume), a red stop square, a white window outline (Open note), and a red recording
/// dot (Keep recording). 4 × 4 supersampled RGBA.
fn glyph(id: u32, size: u32) -> Vec<u8> {
    const SS: u32 = 4;
    const RED: [u8; 3] = [0xf8, 0x54, 0x52];
    const WHITE: [u8; 3] = [0xf4, 0xf2, 0xf0];
    let s = size as f32;
    let inside = |x: f32, y: f32| -> bool {
        match id {
            STOP => {
                // A 56% square with corners rounded by a fifth of its side.
                let (half, r) = (s * 0.28, s * 0.11);
                let (dx, dy) = (((x - s / 2.0).abs() - (half - r)).max(0.0), ((y - s / 2.0).abs() - (half - r)).max(0.0));
                (dx * dx + dy * dy).sqrt() <= r
            }
            PAUSE => {
                // Two bars.
                let (top, bottom, w) = (s * 0.22, s * 0.78, s * 0.16);
                let in_bar = |c: f32| x >= c - w / 2.0 && x <= c + w / 2.0;
                y >= top && y <= bottom && (in_bar(s * 0.36) || in_bar(s * 0.64))
            }
            RESUME => {
                // A play triangle pointing right.
                let (l, r, t, b) = (s * 0.3, s * 0.78, s * 0.2, s * 0.8);
                let half = (b - t) / 2.0 * (1.0 - (x - l) / (r - l));
                x >= l && x <= r && (y - s / 2.0).abs() <= half
            }
            OPEN => {
                // A window outline with a thicker title bar.
                let (l, t, r, b, w) = (s * 0.14, s * 0.2, s * 0.86, s * 0.8, (s * 0.09).max(1.2));
                let within = x >= l && x <= r && y >= t && y <= b;
                let hollow = x > l + w && x < r - w && y > t + w * 2.2 && y < b - w;
                within && !hollow
            }
            _ => ((x - s / 2.0).powi(2) + (y - s / 2.0).powi(2)).sqrt() <= s * 0.3,
        }
    };
    let color = if matches!(id, STOP | KEEP) { RED } else { WHITE };
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let hits = (0..SS * SS)
                .filter(|k| inside(x as f32 + ((k % SS) as f32 + 0.5) / SS as f32, y as f32 + ((k / SS) as f32 + 0.5) / SS as f32))
                .count() as u32;
            let i = ((y * size + x) * 4) as usize;
            rgba[i..i + 3].copy_from_slice(&color);
            rgba[i + 3] = (hits * 255 / (SS * SS)) as u8;
        }
    }
    rgba
}
