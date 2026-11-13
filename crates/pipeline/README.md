# cilica-pipeline

One recording session end to end, independent of Tauri:
capture → echo cancellation → VAD → speech-to-text → store → events → diarize → summarise.

## Public API

`RecordingService::new(db, events, audio: Arc<dyn AudioBackend>, speech: Arc<dyn SpeechBackend>,
summarizer: SummarizerProvider, ServiceConfig, tokio Handle)`, cheap to clone:

- `start(StartRecordingOptions)`: refuses while capturing (`Conflict`); checks the engine first
  (`Unavailable` naming the fix when the speech model is not downloaded or a cloud engine is
  picked); asks for the microphone if never asked (`PermissionDenied` if refused); creates the
  note ("Zoom meeting · 14:05", "Google Meet · 14:05", "Recording · 14:05"; kind Meeting) or
  reuses `note_id` (the new audio and segments continue after the old ones); starts both
  captures. Without the system-audio permission it records the mic only and raises a toast
  that says where to allow it. Non-manual triggers (hotkey, tray, detection) also raise a
  "Recording" toast with Stop.
- `stop()`: returns once capture has stopped and the WAV is closed; the rest runs in the
  background. `toggle(trigger)`, `keep_recording()`, `state()`, `is_capturing()`, `is_busy()`
  (true from start until the summary is written, and while a recovered recording is finished).
- `regenerate_summary(note_id)`.
- `apply_settings(&Settings)`: starts, updates or stops the meeting detector
  (`recording.auto_detect`, `meeting_apps`). `handle_detect_event(DetectEvent)`: emits
  `MeetingDetected` / `MeetingEnded`; on detection while idle, `AskFirst` raises a toast with
  `ToastAction::StartRecording`, `AutoStart` starts recording; when a meeting app that belongs
  to the recording releases the mic, `auto_stop_at` is set 10 s ahead with a "Keep recording"
  toast, and recording stops then unless `keep_recording` was called.
- `startup()`: finishes recordings interrupted by a quit or crash, then `apply_retention()`
  (kept audio older than `recording.audio_retention_days` is deleted, 0 keeps it; transcripts
  stay).

### What a session emits

- `RecordingStateChanged`: Idle → Starting → Recording → Stopping (the last audio is
  transcribed) → Diarizing → Summarizing → Idle. Background work of an older session never
  changes the state of a newer one.
- `AudioLevels` at 20 Hz while capturing (0, 0 at the end).
- `TranscriptSegment`: partials (`is_final: false`) upserted in place by id, then the final.
  Mic segments are speaker "You", system segments "Others". A retracted partial or a mic
  segment found to be echo of the call is deleted and announced with `DataChanged
  { Transcript }`.
- `DataChanged` when the note is created, when capture ends (meeting times), when the audio is
  kept, after diarization (Transcript, People, Notes) and after the summary.
- Toasts: device lost and back, mic only, transcription problems, a voice Cilica may know
  ("Is Person 2 Sam?": never applied silently), summary unavailable or failed (with
  `RegenerateSummary`), recovery at launch.

### Files

`<data_dir>/recordings/<note>-<ms>.wav` (crash-safe stereo WAV, mic left, system right) and a
`.json` sidecar recording how far post-processing got, while working; then
`recordings/<note>.ogg`, stored in `notes.audio_path` as `recordings/<note>.ogg` (relative to
the data dir). The first 700 ms of capture are silenced and audio after the stop instant is
dropped, so start and stop chimes played by the UI after `Recording` and after `Stopping` stay
out of the recording.

### After capture

Partial segments are removed; meeting times set; the audio kept (appended to the note's earlier
audio when recording into an existing note); then diarization on the system channel (Person 1…n)
and, when the mic has 10 s or more of speech, on the mic for people in the room (a second mic
voice counts only with 3 s of speech that does not sound like anyone on the call); speakers'
centroids stored; voiceprint suggestions computed; the note reindexed; the summariser called
(a failure is a toast, never fatal).

## Echo

Two layers: WebRTC AEC3 on the mic with system audio as reference (`cilica-audio`), then a text
check here: a final mic utterance of three or more words, overlapping a system utterance within
1.5 s and sharing at least 60 % of its words with it, is dropped (whichever finished first).

## Must not depend on

`tauri` or `cilica-agent`: the summariser comes in as `Arc<dyn MeetingSummarizer>` through a
provider closure.

## Tests

- `cargo test -p cilica-pipeline` (about 40 s): whole sessions from `tests/fixtures` (a 32 s
  meeting: "You" on the mic with a -16 dB echo of the call, two people on the system channel;
  `make_fixtures.sh` rebuilds it with `say` and `afconvert`) through `ClipSource`s and a
  scripted transcriber: segments, speakers, phases, levels, auto-stop, kept audio, search;
  mic-only with an unplugged mic; missing model; recording twice into one note; recovery of a
  crashed recording; retention; detection (ask first, auto start, keep recording).
- `cargo test --release -p cilica-pipeline -- --ignored --nocapture` runs the same meeting through
  Whisper (tiny by default, `CILICA_TEST_WHISPER=ggml-large-v3-turbo-q5_0.bin` for the default
  model), Silero VAD and the speaker models in `target/test-models/`: word error rate, live lag,
  no echo duplicates, two people on the call, a Saved person suggested and not linked.
