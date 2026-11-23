# RecordingStatus

How a recording shows itself. Inside the app: one quiet control in the note's title band, neutral like the rest of the band. Red (`rec`) is kept for the tray and taskbar outside the app, where it has to stand out among other apps. Outside it: the tray (menu bar) and the taskbar or Dock. Nothing floats over the note or the screen: the top centre is where meeting apps put their own banners and what screen shares pick up, and every recorder people know (macOS screen recording, CleanShot, Loom) lives in the menu bar or tray.

Recording starts from the transcript panel, the sidebar, ⌘⇧R or the tray. The sidebar marks the note being recorded.

## In the app: the recording control

In the note's title band right after the Notes / Transcript switch (so it shows on both panels), only on the note being recorded. It sits in the band's `trail` slot, which starts the end column: the switch stays centred and never moves when the control appears, changes width (Paused, Keep recording) or leaves. A neutral status pill, then two round 28px buttons, Pause and Stop, on `fill-press`. No red anywhere in the app, and nothing pulses: the sidebar's mic on the recording note is `ink-2` too.

**Recording**: the pill shows the mic icon and the elapsed time ("12:04", Geist Mono). Then Pause, then Stop.
**Paused**: the pill empties to a 1px `line-strong` outline, its icon flips to pause and it reads "Paused · 12:04" with the time held; the Pause button's glyph flips to Play ("Resume recording").
**Auto-stop countdown**: the pill reads "Stops in 8s", followed by a small secondary "Keep recording" button, then Pause and Stop. The toast with Keep recording still shows.
**Starting / writing notes**: the pill alone, "Starting…" or "Writing notes…" with the shimmer.

**Stop**: one click stops. While a meeting in the recording is still in its call (`RecordingState.meeting_live`, set from the meeting detector), it asks first: "Stop recording?" / "Zoom still looks like it's in a call. If you stop now, the rest of the meeting won't be recorded or in the notes." with Keep recording (focused) and Stop recording. No dialog once the meeting app has let go (the countdown) or for a recording started by hand with no meeting detected. ⌘⇧R, the tray and the taskbar stop at once.

**Motion**: the control springs in (`bouncy`, from 0.9) and fades out on the exit tween. Icon swaps (mic ⇄ pause, pause ⇄ play) cross-fade with a quarter turn on `bouncy`. Reduce motion drops the turn and keeps the cross-fade.

Built by `RecordingControl` in `src/ui/Recorder.tsx` (`.nt-recctl*` in `ui.css`); `features/recorder/RecordingChip.tsx` feeds it the recorder state and owns the confirm dialog (`stopNeedsConfirm` in `model.ts`).

Pausing drops the audio until you resume: nothing is recorded or transcribed, and the file and transcript carry on from where they stopped, so the elapsed time leaves pauses out.

## Outside the app

**Tray icon**: on macOS a template glyph (the ridgeline mark) whose sun grows while capturing. On Windows and Linux, the app icon with a red dot while capturing. Paused shows the resting icon.

**Menu bar title** (macOS, and Linux panels that support it): "12:04" beside the glyph while recording, "Paused 12:04" while paused, "Stops in 8s" during the countdown. Nothing while idle or writing notes.

**Tooltip**: "Cilica · Recording 12:04", "Cilica · Paused at 12:04", "Cilica · Stops in 8s", "Cilica · Writing notes…", or "Cilica".

**Tray menu** (native, no icons):

| State | Items |
| --- | --- |
| Idle | Open Cilica · Start recording · — · ✓ Listening for meetings · — · Quit Cilica |
| Recording | *Recording · 12:04* · — · Pause · Stop and write notes · — · Open note · — · ✓ Listening for meetings · — · Quit Cilica |
| Paused | *Paused · 12:04* · — · Resume · Stop and write notes · — · Open note · — · ✓ Listening for meetings · — · Quit Cilica |
| Countdown | *Meeting ended · stops in 8s* · — · Keep recording · Pause · Stop and write notes · — · Open note · — · ✓ Listening for meetings · — · Quit Cilica |
| Writing notes | *Writing notes…* · — · Open note · — · ✓ Listening for meetings · — · Quit Cilica |

Italic lines are disabled status lines; — is a separator; ✓ Listening for meetings is a check item. Open note shows and focuses the main window on the recording's note.

**macOS Dock**: a badge with the elapsed time while recording ("Paused" while paused); the icon's progress bar runs (indeterminate) while Cilica writes the notes; one bounce when a detected meeting ends and the countdown starts.

**Windows taskbar button**: a red dot overlay while capturing (gone while paused); indeterminate progress while writing notes; the button flashes when the countdown starts. The window title becomes "Recording 12:04 · Cilica", so the hover label shows it. The hover thumbnail has a toolbar: Pause (Resume while paused), Stop and write notes, Open note, Keep recording (Keep recording only during the countdown; Pause and Stop hide while writing notes). The button only exists while the main window is shown, so the tray is the one surface that is always there.

**Linux**: tray icon and menu as above; progress only on desktops with libunity.

**Implementation**: `src-tauri/src/shell/status.rs` turns the recording state and the clock into every label (pure, unit-tested); `tray.rs`, `taskbar.rs` and `thumbbar.rs` (Windows, `ITaskbarList3`) apply them. A once-a-second ticker refreshes them while a recording is live.
