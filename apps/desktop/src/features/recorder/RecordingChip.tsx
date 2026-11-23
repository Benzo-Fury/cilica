/**
 * The recording control in the note's title band, right after the Notes / Transcript switch, on
 * the note being recorded (RecordingStatus doc). Neutral, like the rest of the band: red is kept
 * for the tray and taskbar outside the app. A pill says what is happening ("12:04", "Paused ·
 * 12:04", "Stops in 8s", "Writing notes…"), then round Pause and Stop. Stop asks first while a
 * meeting in the recording is still in its call. Outside the app the tray, menu bar and taskbar
 * say the same (src-tauri/src/shell/status.rs).
 */
import { AnimatePresence } from "motion/react";
import { useState } from "react";
import { useShortcut } from "@/app/keybinds";
import { clock } from "@/lib/format";
import { ConfirmDialog, RecordingControl, type RecordingControlStatus } from "@/ui";
import {
  type RecorderMode,
  recorderMode,
  stopNeedsConfirm,
  useClock,
  useKeepRecording,
  usePauseRecording,
  useRecordingState,
  useStopRecording,
} from "./hooks";

export function RecordingChip({ noteId }: { noteId: string }) {
  const state = useRecordingState();
  const ours = state.phase !== "idle" && state.note_id === noteId;
  const ticking = ours && state.paused_at === null;
  const now = useClock(ticking, state.auto_stop_at ? 250 : 1000);
  const stop = useStopRecording();
  const keep = useKeepRecording();
  const pause = usePauseRecording();
  const recordKeys = useShortcut("recording.toggle");
  const [confirming, setConfirming] = useState(false);
  const status = ours ? controlStatus(recorderMode(state, now)) : null;
  const meeting = ours ? stopNeedsConfirm(state) : null;

  return (
    <>
      <AnimatePresence initial={false}>
        {status && (
          <RecordingControl
            key="recording"
            status={status}
            onPause={() => void pause()}
            onStop={() => (meeting ? setConfirming(true) : void stop())}
            onKeep={() => void keep()}
            stopShortcut={recordKeys}
          />
        )}
      </AnimatePresence>
      <ConfirmDialog
        open={confirming && meeting !== null}
        onOpenChange={setConfirming}
        title="Stop recording?"
        body={`${meeting ?? "The meeting"} still looks like it's in a call. If you stop now, the rest of the meeting won't be recorded or in the notes.`}
        confirmLabel="Stop recording"
        cancelLabel="Keep recording"
        initialFocus="cancel"
        onConfirm={() => void stop()}
      />
    </>
  );
}

/** What the control shows for a recorder mode; null when there is nothing to show. */
function controlStatus(mode: RecorderMode): RecordingControlStatus | null {
  switch (mode.kind) {
    case "idle":
      return null;
    case "starting":
      return { kind: "starting" };
    case "finishing":
      return { kind: "finishing", label: mode.label };
    case "recording":
      return {
        kind: "live",
        elapsed: clock(mode.elapsedMs),
        paused: mode.paused,
        stopsIn: mode.autoStopInMs === null ? null : Math.ceil(mode.autoStopInMs / 1000),
      };
  }
}
