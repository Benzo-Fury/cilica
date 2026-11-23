# RecordingStatus

How a recording shows itself. Inside the app: one chip in the note header. Outside it: the tray (menu bar) and the taskbar or Dock. Nothing floats over the note or the screen: the top centre is where meeting apps put their own banners and what screen shares pick up, and every recorder people know (macOS screen recording, CleanShot, Loom) lives in the menu bar or tray.

Recording starts from the transcript panel, the sidebar, ⌘⇧R or the tray. The sidebar marks the note being recorded.

## In the app: the recording chip

`nt-chip-rec`, a `<button>` in the note's meta row, only on the note being recorded.

**Recording**: breathing `rec` dot, "Recording · 12:04" (Geist Mono timer). A click stops the recording. Tooltip "Stop recording" with ⌘⇧R.
**Auto-stop countdown**: "Stops in 8s". A click keeps recording. Tooltip "Keep recording". The toast with Keep recording still shows.
**Writing notes**: "Writing notes…" with the shimmer, on `fill-press`, not clickable (`nt-chip-busy`).

## Outside the app

**Tray icon**: on macOS a template glyph (the ridgeline mark) whose sun grows while recording. On Windows and Linux, the app icon with a red dot while recording.

**Menu bar title** (macOS, and Linux panels that support it): "12:04" beside the glyph while recording, "Stops in 8s" during the countdown. Nothing while idle or writing notes.

**Tooltip**: "Cilica · Recording 12:04", "Cilica · Stops in 8s", "Cilica · Writing notes…", or "Cilica".

**Tray menu** (native, no icons):

| State | Items |
| --- | --- |
| Idle | Open Cilica · Start recording · — · ✓ Listening for meetings · — · Quit Cilica |
| Recording | *Recording · 12:04* · Open note · Stop and write notes · — · ✓ Listening for meetings · — · Quit Cilica |
| Countdown | *Meeting ended · stops in 8s* · Open note · Keep recording · Stop and write notes · — · ✓ Listening for meetings · — · Quit Cilica |
| Writing notes | *Writing notes…* · Open note · — · ✓ Listening for meetings · — · Quit Cilica |

Italic lines are disabled status lines; — is a separator; ✓ Listening for meetings is a check item. Open note shows and focuses the main window on the recording's note.

**macOS Dock**: a badge with the elapsed time while recording; the icon's progress bar runs (indeterminate) while Cilica writes the notes; one bounce when a detected meeting ends and the countdown starts.

**Windows taskbar button**: a red dot overlay while recording; indeterminate progress while writing notes; the button flashes when the countdown starts. The window title becomes "Recording 12:04 · Cilica", so the hover label shows it. The hover thumbnail has a toolbar: Stop and write notes, Open note, Keep recording (Keep recording only during the countdown; Stop hides while writing notes). The button only exists while the main window is shown, so the tray is the one surface that is always there.

**Linux**: tray icon and menu as above; progress only on desktops with libunity.

**Implementation**: `src-tauri/src/shell/status.rs` turns the recording state and the clock into every label (pure, unit-tested); `tray.rs`, `taskbar.rs` and `thumbbar.rs` (Windows, `ITaskbarList3`) apply them. A once-a-second ticker refreshes them while a recording is live.
