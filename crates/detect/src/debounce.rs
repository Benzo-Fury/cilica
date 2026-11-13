//! Turning a once-a-second "who holds the mic" snapshot into Detected / Ended events.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use cilica_domain::MeetingDetected;

use crate::{DetectEvent, Identified};

#[derive(Debug, Clone, Copy)]
pub struct DebounceConfig {
    /// How long an app must hold the mic before it counts as a meeting.
    pub hold: Duration,
    /// How long it must let go before the meeting counts as ended.
    pub release: Duration,
    /// Blips shorter than this while holding do not restart the hold timer.
    pub grace: Duration,
}

impl Default for DebounceConfig {
    fn default() -> Self {
        Self { hold: Duration::from_secs(5), release: Duration::from_secs(15), grace: Duration::from_secs(3) }
    }
}

struct Track {
    app_id: Option<String>,
    since: Instant,
    last_seen: Instant,
    reported: bool,
}

pub struct Debouncer {
    cfg: DebounceConfig,
    apps: HashMap<String, Track>,
}

impl Debouncer {
    pub fn new(cfg: DebounceConfig) -> Self {
        Self { cfg, apps: HashMap::new() }
    }

    pub fn update(&mut self, now: Instant, seen: &[Identified]) -> Vec<DetectEvent> {
        let mut out = Vec::new();
        for s in seen {
            let t = self.apps.entry(s.app_name.clone()).or_insert(Track {
                app_id: s.app_id.clone(),
                since: now,
                last_seen: now,
                reported: false,
            });
            if !t.reported && now.duration_since(t.last_seen) > self.cfg.grace {
                t.since = now; // it was gone too long: start the hold again
            }
            t.last_seen = now;
            if !t.reported && now.duration_since(t.since) >= self.cfg.hold {
                t.reported = true;
                out.push(DetectEvent::Detected(MeetingDetected { app_name: s.app_name.clone(), app_id: t.app_id.clone() }));
            }
        }
        let mut gone = Vec::new();
        for (name, t) in &self.apps {
            let missing = !seen.iter().any(|s| &s.app_name == name);
            if missing {
                let away = now.duration_since(t.last_seen);
                if t.reported && away >= self.cfg.release {
                    out.push(DetectEvent::Ended { app_name: name.clone() });
                    gone.push(name.clone());
                } else if !t.reported && away > self.cfg.grace {
                    gone.push(name.clone());
                }
            }
        }
        for g in gone {
            self.apps.remove(&g);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zoom() -> Identified {
        Identified { app_name: "Zoom".into(), app_id: Some("us.zoom.xos".into()) }
    }

    #[test]
    fn five_seconds_to_detect_fifteen_to_end() {
        let mut d = Debouncer::new(DebounceConfig::default());
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        for s in 0..5 {
            assert!(d.update(at(s), &[zoom()]).is_empty(), "nothing before 5 s ({s})");
        }
        let ev = d.update(at(5), &[zoom()]);
        assert_eq!(ev, vec![DetectEvent::Detected(MeetingDetected { app_name: "Zoom".into(), app_id: Some("us.zoom.xos".into()) })]);
        assert!(d.update(at(6), &[zoom()]).is_empty(), "reported once");
        // Released at 7 s: nothing until 15 s later.
        for s in 7..21 {
            assert!(d.update(at(s), &[]).is_empty(), "still within release window at {s}");
        }
        assert_eq!(d.update(at(21), &[]), vec![DetectEvent::Ended { app_name: "Zoom".into() }]);
        assert!(d.update(at(22), &[]).is_empty());
    }

    #[test]
    fn short_grabs_and_blips() {
        let mut d = Debouncer::new(DebounceConfig::default());
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        // Held for 3 s then gone: never reported.
        for s in 0..3 {
            d.update(at(s), &[zoom()]);
        }
        for s in 3..30 {
            assert!(d.update(at(s), &[]).is_empty());
        }
        // A one-second blip while holding does not reset the hold.
        d.update(at(40), &[zoom()]);
        d.update(at(41), &[zoom()]);
        d.update(at(42), &[]);
        d.update(at(43), &[zoom()]);
        d.update(at(44), &[zoom()]);
        assert_eq!(d.update(at(45), &[zoom()]).len(), 1);
        // Coming back within the release window cancels the end.
        for s in 46..55 {
            assert!(d.update(at(s), &[]).is_empty());
        }
        assert!(d.update(at(55), &[zoom()]).is_empty());
        for s in 56..70 {
            assert!(d.update(at(s), &[]).is_empty());
        }
        assert_eq!(d.update(at(70), &[]).len(), 1);
    }
}
