/**
 * The installer window's flow: the first screen, then progress, then done. Progress comes from
 * Cilica Setup as events (`JobEvent`); this folds them into what the progress screen shows.
 */
import type { JobEvent, Mode, Step } from "@/ipc/installer/client";
import type { AppError } from "@/ipc/types";

export type Screen = "start" | "working" | "done";

export interface JobState {
  screen: Screen;
  /** Every step of the running job, in order. */
  steps: Step[];
  /** The step under way, or the one that failed. */
  current: Step | null;
  /** Finished steps, in the order they finished. */
  done: Step[];
  /** The whole job, 0..1. Never moves back. */
  progress: number;
  error: AppError | null;
}

export type JobAction =
  /** The job was asked for; Setup may report before it answers. */
  | { type: "starting" }
  /** Setup answered with the job's steps. */
  | { type: "started"; steps: Step[] }
  | { type: "event"; event: JobEvent }
  /** The job could not start, or the window lost Setup. */
  | { type: "refused"; error: AppError }
  /** Back to the first screen after a failure. */
  | { type: "retry" };

export const initialJob: JobState = {
  screen: "start",
  steps: [],
  current: null,
  done: [],
  progress: 0,
  error: null,
};

export function jobReducer(state: JobState, action: JobAction): JobState {
  switch (action.type) {
    case "starting":
      return { ...initialJob, screen: "working" };
    case "started":
      // Keep what Setup already reported; only a job still at its start needs a first step.
      return state.screen === "working" && !state.error
        ? { ...state, steps: action.steps, current: state.current ?? action.steps[0] ?? null }
        : state;
    case "refused":
      return { ...state, screen: "working", error: action.error };
    case "retry":
      return initialJob;
    case "event":
      return applyEvent(state, action.event);
  }
}

function applyEvent(state: JobState, event: JobEvent): JobState {
  // Events after a failure or the finish belong to nothing on screen.
  if (state.screen !== "working" || state.error) return state;
  const progress = (value: number | null) => Math.max(state.progress, value ?? 0);
  switch (event.type) {
    case "step":
      return { ...state, current: event.step, progress: progress(event.progress) };
    case "step_done":
      return {
        ...state,
        current: nextStep(state.steps, event.step),
        done: state.done.includes(event.step) ? state.done : [...state.done, event.step],
        progress: progress(event.progress),
      };
    case "finished":
      return { ...state, screen: "done", current: null, progress: 1 };
    case "failed":
      return { ...state, error: event.error };
  }
}

function nextStep(steps: Step[], step: Step): Step | null {
  const index = steps.indexOf(step);
  return index >= 0 ? (steps[index + 1] ?? null) : step;
}

/** Whether Setup is busy: the window's close button waits until it is not. */
export function isBusy(state: JobState): boolean {
  return state.screen === "working" && !state.error;
}

/** The sun's height when Cilica is in (`SceneArt`'s `sun`): clear of the ridges in this short window. */
const SUN_UP = 1.6;
/** Its height before Cilica is in, and after it is removed: just over the ridges. */
const SUN_DOWN = 0.2;

/**
 * How high the sun stands: it rises as Cilica installs or updates and sets as it is removed, so
 * the art tells the same story as the bar.
 */
export function sunHeight(mode: Mode, state: JobState): number {
  const done = state.screen === "done" ? 1 : state.screen === "working" ? state.progress : 0;
  return mode === "uninstall"
    ? SUN_UP - (SUN_UP - SUN_DOWN) * done
    : SUN_DOWN + (SUN_UP - SUN_DOWN) * done;
}
