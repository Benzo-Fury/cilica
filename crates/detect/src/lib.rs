//! Meeting detection: notice when an allow-listed meeting app holds the microphone.
//!
//! A background thread polls, once a second, which processes are capturing audio
//! ([`mic_users`], per OS), maps them to display names ([`apps::identify`]), keeps those on the
//! allow-list, and debounces ([`Debouncer`]): an app must hold the mic for 5 s before
//! `MeetingDetected`, and release it for 15 s before `MeetingEnded`. Margin itself is never
//! reported. Browsers count only when a meeting window title can be read (macOS, with the
//! optional Accessibility permission).

pub mod apps;
mod debounce;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod pactl;
#[cfg(target_os = "windows")]
mod windows;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub use apps::{Identified, identify, matches_allow_list};
pub use debounce::{DebounceConfig, Debouncer};
use margin_domain::MeetingDetected;

/// A process that is capturing audio right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicUser {
    pub pid: u32,
    /// Bundle id (macOS), executable name (Windows) or `application.name` (Linux).
    pub id: String,
    /// Window titles, when they could be read (browsers on macOS with Accessibility).
    pub window_titles: Vec<String>,
}

/// What the detector reports.
#[derive(Debug, Clone, PartialEq)]
pub enum DetectEvent {
    Detected(MeetingDetected),
    Ended { app_name: String },
}

/// Processes capturing from any microphone right now, excluding this process.
pub fn mic_users(read_window_titles: bool) -> Vec<MicUser> {
    #[cfg(target_os = "macos")]
    let users = macos::mic_users(read_window_titles);
    #[cfg(target_os = "windows")]
    let users = windows::mic_users(read_window_titles);
    #[cfg(target_os = "linux")]
    let users = linux::mic_users(read_window_titles);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    let users: Vec<MicUser> = {
        let _ = read_window_titles;
        Vec::new()
    };
    let me = std::process::id();
    users.into_iter().filter(|u| u.pid != me && !apps::is_margin(&u.id)).collect()
}

/// Whether window titles may be read (macOS Accessibility granted). Never prompts.
pub fn can_read_window_titles() -> bool {
    #[cfg(target_os = "macos")]
    return cidre::ax::is_process_trusted();
    #[cfg(not(target_os = "macos"))]
    false
}

/// The allow-listed meetings visible right now, deduplicated by display name.
pub fn current_meetings(allow: &[String]) -> Vec<Identified> {
    let titles = can_read_window_titles();
    let mut out: Vec<Identified> = Vec::new();
    for u in mic_users(titles) {
        if let Some(found) = identify(&u)
            && matches_allow_list(&found.app_name, allow)
            && !out.iter().any(|o| o.app_name == found.app_name)
        {
            out.push(found);
        }
    }
    out
}

const POLL: Duration = Duration::from_secs(1);

/// The running detector. Dropping it stops the thread.
pub struct MeetingDetector {
    stop: Arc<AtomicBool>,
    allow: Arc<Mutex<Vec<String>>>,
    wake: mpsc::Sender<()>,
    handle: Option<JoinHandle<()>>,
}

impl MeetingDetector {
    /// Start polling. `on_event` is called from the detector thread.
    pub fn start(allow: Vec<String>, on_event: impl Fn(DetectEvent) + Send + 'static) -> Self {
        Self::start_with(allow, DebounceConfig::default(), on_event)
    }

    pub fn start_with(allow: Vec<String>, cfg: DebounceConfig, on_event: impl Fn(DetectEvent) + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let allow = Arc::new(Mutex::new(allow));
        let (wake, wake_rx) = mpsc::channel::<()>();
        let (t_stop, t_allow) = (stop.clone(), allow.clone());
        let handle = std::thread::Builder::new()
            .name("margin-detect".into())
            .spawn(move || {
                let mut deb = Debouncer::new(cfg);
                while !t_stop.load(Ordering::SeqCst) {
                    let allow = t_allow.lock().map(|a| a.clone()).unwrap_or_default();
                    let seen = current_meetings(&allow);
                    for ev in deb.update(Instant::now(), &seen) {
                        tracing::info!(?ev, "meeting detection");
                        on_event(ev);
                    }
                    match wake_rx.recv_timeout(POLL) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .ok();
        Self { stop, allow, wake, handle }
    }

    /// Replace the allow-list (Settings changed).
    pub fn set_allow_list(&self, allow: Vec<String>) {
        if let Ok(mut a) = self.allow.lock() {
            *a = allow;
        }
        let _ = self.wake.send(());
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.wake.send(());
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for MeetingDetector {
    fn drop(&mut self) {
        self.shutdown();
    }
}
