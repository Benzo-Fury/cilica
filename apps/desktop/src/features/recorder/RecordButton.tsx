/**
 * The record control in a note's title band, beside the Notes / Transcript switch:
 * a microphone and "Record meeting" → starting → live (a red stop square and the timer; a click
 * stops, and the square says so without a word) → "Writing notes…".
 * When the band is narrow it shrinks to its icon (see `.nt-recbtn` in ui.css).
 */
import { Mic } from "lucide-react";
import { useShortcut } from "@/app/keybinds";
import { clock } from "@/lib/format";
import { cn, Icon, Shimmer, Timecode, Tooltip } from "@/ui";
import {
  capsuleMode,
  useClock,
  useRecordingState,
  useStopRecording,
  useToggleRecording,
} from "./hooks";

export function RecordButton() {
  const state = useRecordingState();
  const now = useClock(state.phase !== "idle");
  const mode = capsuleMode(state, now);
  const toggle = useToggleRecording();
  const stop = useStopRecording();
  const keys = useShortcut("recording.toggle");
  const recording = mode.kind === "recording";
  const busy = mode.kind === "starting" || mode.kind === "finishing";
  const label = recording
    ? "Stop recording"
    : mode.kind === "starting"
      ? "Starting the recording"
      : mode.kind === "finishing"
        ? "Writing notes"
        : "Start recording";

  return (
    <Tooltip label={label} shortcut={busy ? undefined : keys} side="bottom">
      <button
        type="button"
        className={cn("nt-btn nt-btn-sm nt-btn-secondary nt-recbtn", recording && "nt-recbtn-live")}
        aria-label={label}
        aria-keyshortcuts={keys.join("+")}
        aria-disabled={busy}
        onClick={() => {
          if (busy) return;
          void (recording ? stop() : toggle("manual"));
        }}
      >
        {recording ? (
          <span className="nt-recbtn-stop" aria-hidden />
        ) : (
          <Icon icon={Mic} size="sm" motion="mic" />
        )}
        <span className="nt-recbtn-text">
          {mode.kind === "idle" && "Record meeting"}
          {mode.kind === "starting" && <Shimmer>Starting…</Shimmer>}
          {recording && <Timecode>{clock(mode.elapsedMs)}</Timecode>}
          {mode.kind === "finishing" && <Shimmer>{mode.label}</Shimmer>}
        </span>
      </button>
    </Tooltip>
  );
}
