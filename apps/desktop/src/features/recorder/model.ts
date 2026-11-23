/** What the recording chip shows for a recording state (pure, so it is easy to test). */
import type { RecordingState } from "@/ipc/types";

export type RecorderMode =
  | { kind: "idle" }
  | { kind: "starting" }
  | {
      kind: "recording";
      elapsedMs: number;
      /** Paused: nothing is captured and the time holds. */
      paused: boolean;
      /** Set while a detected meeting has ended: ms until recording stops on its own. */
      autoStopInMs: number | null;
    }
  | { kind: "finishing"; label: string; step: "stopping" | "summarizing" };

export const isFinishing = (state: RecordingState) =>
  state.phase === "stopping" || state.phase === "summarizing";

/** Time recorded so far, leaving out pauses (`RecordingState::elapsed_ms` in the domain crate). */
export function elapsedMs(state: RecordingState, now: number): number {
  const started = state.started_at ? Date.parse(state.started_at) : Number.NaN;
  if (Number.isNaN(started)) return 0;
  const pausedAt = state.paused_at ? Date.parse(state.paused_at) : Number.NaN;
  const until = Number.isNaN(pausedAt) ? now : pausedAt;
  return Math.max(0, until - started - state.paused_ms);
}

export function recorderMode(state: RecordingState, now: number): RecorderMode {
  switch (state.phase) {
    case "idle":
      return { kind: "idle" };
    case "starting":
      return { kind: "starting" };
    case "recording": {
      const stopAt = state.auto_stop_at ? Date.parse(state.auto_stop_at) : null;
      return {
        kind: "recording",
        elapsedMs: elapsedMs(state, now),
        paused: state.paused_at !== null,
        autoStopInMs: stopAt === null || Number.isNaN(stopAt) ? null : Math.max(0, stopAt - now),
      };
    }
    case "stopping":
    case "summarizing":
      // The design shows one message for the whole wrap-up; the step is for screen readers.
      return { kind: "finishing", label: "Writing notes…", step: state.phase };
  }
}

/**
 * The meeting app to name when Stop should ask first: while recording, a meeting in this
 * recording is still in its call (the backend's `meeting_live`). Null: stop at once.
 */
export function stopNeedsConfirm(state: RecordingState): string | null {
  if (state.phase !== "recording") return null;
  return state.meeting_live?.trim() || null;
}
