//! What the tray, menu bar and taskbar say about the recording, worked out from its state and the
//! clock (docs/design-system/components/RecordingStatus). Pure, so it is tested without a window.

use cilica_domain::{RecordingPhase, RecordingState, Timestamp};

/// The recording as the tray and taskbar show it. Each mode has its own tray menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Idle,
    /// Starting or capturing.
    Recording,
    /// Capturing, but the meeting app let go of the mic: it stops on its own unless kept.
    Ending,
    /// Capture stopped; Cilica is finishing the transcript and writing the notes.
    Finishing,
}

impl Mode {
    /// Audio is being captured.
    pub fn is_live(self) -> bool {
        matches!(self, Mode::Recording | Mode::Ending)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub mode: Mode,
    /// The recording has a note to open.
    pub has_note: bool,
    /// Live but paused: nothing is being captured.
    pub paused: bool,
    /// Next to the menu bar glyph (macOS and the Linux panels that show it). `None` shows the glyph alone.
    pub tray_title: Option<String>,
    pub tooltip: String,
    /// The first, disabled line of the tray menu. `None` while idle.
    pub headline: Option<String>,
    /// The Dock badge (macOS).
    pub badge: Option<String>,
    /// The main window's title, which the Windows taskbar shows on hover.
    pub window_title: String,
}

impl Status {
    pub fn idle() -> Self {
        status(&RecordingState::idle(), cilica_domain::now())
    }

    /// Audio is being captured right now (live and not paused).
    pub fn capturing(&self) -> bool {
        self.mode.is_live() && !self.paused
    }
}

pub fn status(state: &RecordingState, now: Timestamp) -> Status {
    let has_note = state.note_id.is_some();
    match state.phase {
        RecordingPhase::Idle => Status {
            mode: Mode::Idle,
            has_note: false,
            paused: false,
            tray_title: None,
            tooltip: "Cilica".into(),
            headline: None,
            badge: None,
            window_title: "Cilica".into(),
        },
        RecordingPhase::Stopping | RecordingPhase::Summarizing => Status {
            mode: Mode::Finishing,
            has_note,
            paused: false,
            tray_title: None,
            tooltip: "Cilica · Writing notes…".into(),
            headline: Some("Writing notes…".into()),
            badge: None,
            window_title: "Writing notes… · Cilica".into(),
        },
        RecordingPhase::Starting | RecordingPhase::Recording => {
            let elapsed = clock(state.elapsed_ms(now));
            let paused = state.paused_at.is_some();
            if let Some(stop_at) = state.auto_stop_at {
                let left = seconds_left((stop_at - now).num_milliseconds());
                let stops = format!("Stops in {left}s");
                return Status {
                    mode: Mode::Ending,
                    has_note,
                    paused,
                    tooltip: format!("Cilica · {stops}"),
                    headline: Some(format!("Meeting ended · stops in {left}s")),
                    badge: Some(elapsed),
                    window_title: format!("{stops} · Cilica"),
                    tray_title: Some(stops),
                };
            }
            if paused {
                return Status {
                    mode: Mode::Recording,
                    has_note,
                    paused,
                    tray_title: Some(format!("Paused {elapsed}")),
                    tooltip: format!("Cilica · Paused at {elapsed}"),
                    headline: Some(format!("Paused · {elapsed}")),
                    badge: Some("Paused".into()),
                    window_title: format!("Paused {elapsed} · Cilica"),
                };
            }
            let starting = state.phase == RecordingPhase::Starting;
            Status {
                mode: Mode::Recording,
                has_note,
                paused,
                tray_title: Some(elapsed.clone()),
                tooltip: format!("Cilica · Recording {elapsed}"),
                headline: Some(if starting { "Starting…".into() } else { format!("Recording · {elapsed}") }),
                window_title: format!("Recording {elapsed} · Cilica"),
                badge: Some(elapsed),
            }
        }
    }
}

/// Whole seconds left, rounded up, never below zero.
fn seconds_left(ms: i64) -> i64 {
    (ms.max(0) + 999) / 1000
}

/// `m:ss`, or `h:mm:ss` from an hour, like the app's own clock (`src/lib/format.ts`).
pub fn clock(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use cilica_domain::{RecordingPhase, RecordingState};

    use super::*;

    fn at(secs: i64) -> Timestamp {
        Utc.timestamp_opt(1_800_000_000 + secs, 0).unwrap()
    }

    fn live(phase: RecordingPhase) -> RecordingState {
        RecordingState { phase, note_id: Some(cilica_domain::new_id()), started_at: Some(at(0)), ..RecordingState::idle() }
    }

    #[test]
    fn clock_matches_the_app() {
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(-5), "0:00");
        assert_eq!(clock(724_900), "12:04");
        assert_eq!(clock(3_725_000), "1:02:05");
    }

    #[test]
    fn idle_says_nothing() {
        let s = status(&RecordingState::idle(), at(0));
        assert_eq!(s.mode, Mode::Idle);
        assert_eq!((s.tray_title, s.headline, s.badge), (None, None, None));
        assert_eq!(s.tooltip, "Cilica");
        assert_eq!(s.window_title, "Cilica");
    }

    #[test]
    fn recording_shows_the_time_everywhere() {
        let s = status(&live(RecordingPhase::Recording), at(724));
        assert_eq!(s.mode, Mode::Recording);
        assert!(s.has_note);
        assert_eq!(s.tray_title.as_deref(), Some("12:04"));
        assert_eq!(s.tooltip, "Cilica · Recording 12:04");
        assert_eq!(s.headline.as_deref(), Some("Recording · 12:04"));
        assert_eq!(s.badge.as_deref(), Some("12:04"));
        assert_eq!(s.window_title, "Recording 12:04 · Cilica");
    }

    #[test]
    fn starting_says_so() {
        let s = status(&RecordingState { started_at: None, ..live(RecordingPhase::Starting) }, at(3));
        assert_eq!(s.mode, Mode::Recording);
        assert_eq!(s.headline.as_deref(), Some("Starting…"));
        assert_eq!(s.tray_title.as_deref(), Some("0:00"));
    }

    #[test]
    fn auto_stop_counts_down_in_whole_seconds() {
        let state = RecordingState { auto_stop_at: Some(at(60) + Duration::milliseconds(7_200)), ..live(RecordingPhase::Recording) };
        let s = status(&state, at(60));
        assert_eq!(s.mode, Mode::Ending);
        assert!(s.mode.is_live());
        assert_eq!(s.tray_title.as_deref(), Some("Stops in 8s"));
        assert_eq!(s.headline.as_deref(), Some("Meeting ended · stops in 8s"));
        assert_eq!(s.badge.as_deref(), Some("1:00"));
        assert_eq!(status(&state, at(70)).tray_title.as_deref(), Some("Stops in 0s"));
    }

    #[test]
    fn paused_holds_the_clock() {
        let state = RecordingState { paused_at: Some(at(300)), paused_ms: 60_000, ..live(RecordingPhase::Recording) };
        let s = status(&state, at(900));
        assert!(s.paused && s.mode.is_live() && !s.capturing());
        assert_eq!(s.tray_title.as_deref(), Some("Paused 4:00"));
        assert_eq!(s.headline.as_deref(), Some("Paused · 4:00"));
        assert_eq!(s.badge.as_deref(), Some("Paused"));
        // Resumed: the pause is left out of the time.
        let resumed = RecordingState { paused_at: None, paused_ms: 660_000, ..state };
        assert_eq!(status(&resumed, at(900)).tray_title.as_deref(), Some("4:00"));
    }

    #[test]
    fn finishing_writes_notes() {
        for phase in [RecordingPhase::Stopping, RecordingPhase::Summarizing] {
            let s = status(&live(phase), at(900));
            assert_eq!(s.mode, Mode::Finishing);
            assert!(!s.mode.is_live());
            assert_eq!(s.tray_title, None);
            assert_eq!(s.badge, None);
            assert_eq!(s.headline.as_deref(), Some("Writing notes…"));
        }
    }
}
