# cilica-detect

Meeting detection: notices when an allow-listed meeting app holds the microphone.

## Public API

- `MeetingDetector::start(allow_list, on_event)`: a thread polls once a second which processes
  capture audio (`mic_users`), names them (`identify`: bundle ids, executables, PipeWire
  `application.name`; browsers only from a meeting window title, read through Accessibility on
  macOS when that optional permission is granted), keeps those on the allow-list, and debounces
  (`Debouncer`): `DetectEvent::Detected` after 5 s holding the mic, `DetectEvent::Ended` after
  15 s released. Cilica itself is never reported. `set_allow_list`, `stop` (or drop).
- Per OS: macOS reads Core Audio process objects (`kAudioProcessPropertyIsRunningInput`,
  macOS 14+); Windows enumerates capture sessions on the default mic (unverified); Linux parses
  `pactl list source-outputs` (unverified).

## Must not depend on

`tauri`, `cilica-store`, or any app crate except `cilica-domain`.

## Tests

`cargo test -p cilica-detect`: debouncing, app identification and allow-list matching, `pactl`
parsing. `cargo run -p cilica-detect --example detector` prints what the detector sees live.
