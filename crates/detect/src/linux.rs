//! Linux: PulseAudio / PipeWire-Pulse source outputs (streams recording from a source), read
//! with `pactl list source-outputs`. Unverified: written without a Linux machine to run it on.

use crate::MicUser;

pub(crate) fn mic_users(_read_window_titles: bool) -> Vec<MicUser> {
    let Ok(out) = std::process::Command::new("pactl").args(["list", "source-outputs"]).env("LC_ALL", "C").output() else {
        return Vec::new();
    };
    crate::pactl::parse(&String::from_utf8_lossy(&out.stdout))
}
