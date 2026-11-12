/** What the recorder capsule shows for a recording state (pure, so it is easy to test). */
import type { RecordingState } from "@/ipc/types";

export type CapsuleMode =
  | { kind: "idle" }
  | { kind: "starting" }
  | {
      kind: "recording";
      elapsedMs: number;
      /** Set while a detected meeting has ended: ms until recording stops on its own. */
      autoStopInMs: number | null;
    }
  | { kind: "finishing"; label: string; step: "stopping" | "diarizing" | "summarizing" };

export const isFinishing = (state: RecordingState) =>
  state.phase === "stopping" || state.phase === "diarizing" || state.phase === "summarizing";

export function capsuleMode(state: RecordingState, now: number): CapsuleMode {
  switch (state.phase) {
    case "idle":
      return { kind: "idle" };
    case "starting":
      return { kind: "starting" };
    case "recording": {
      const started = state.started_at ? Date.parse(state.started_at) : now;
      const stopAt = state.auto_stop_at ? Date.parse(state.auto_stop_at) : null;
      return {
        kind: "recording",
        elapsedMs: Math.max(0, now - (Number.isNaN(started) ? now : started)),
        autoStopInMs: stopAt === null || Number.isNaN(stopAt) ? null : Math.max(0, stopAt - now),
      };
    }
    case "stopping":
    case "diarizing":
    case "summarizing":
      // The design shows one message for the whole wrap-up; the step is for screen readers.
      return { kind: "finishing", label: "Writing notes…", step: state.phase };
  }
}
