/**
 * The setup window's order of things: a welcome screen, then five steps that open one at a time.
 * Finished steps collapse to a line and can be reopened; later steps stay hidden until reached.
 * The last step's button opens Margin, so there is no ending page.
 */

export const STEPS = ["permissions", "speech", "model", "import", "look"] as const;
export type StepId = (typeof STEPS)[number];

export interface FlowState {
  phase: "welcome" | "steps";
  /** Index into STEPS of the open step. */
  current: number;
  /** Steps left with Skip (or Continue without doing anything); their check turns grey. */
  skipped: Partial<Record<StepId, true>>;
}

export type FlowAction =
  | { type: "start" }
  | { type: "next"; skipped: boolean }
  | { type: "reopen"; index: number }
  | { type: "welcome" };

export const initialFlow: FlowState = { phase: "welcome", current: 0, skipped: {} };

export function flowReducer(state: FlowState, action: FlowAction): FlowState {
  switch (action.type) {
    case "start":
      return { ...state, phase: "steps" };
    case "welcome":
      return { ...state, phase: "welcome" };
    case "next": {
      const id = STEPS[state.current];
      if (!id) return state;
      const skipped = { ...state.skipped };
      if (action.skipped) skipped[id] = true;
      else delete skipped[id];
      // The last step has nowhere to go: its button finishes setup instead.
      const current = Math.min(STEPS.length - 1, state.current + 1);
      return { phase: "steps", current, skipped };
    }
    case "reopen":
      if (action.index < 0 || action.index >= state.current) return state;
      return { ...state, phase: "steps", current: action.index };
  }
}

/** How a step shows: open, collapsed to its line, or not yet reached (not rendered). */
export function stepStatus(state: FlowState, index: number): "done" | "active" | "upcoming" {
  if (index < state.current) return "done";
  if (index === state.current) return "active";
  return "upcoming";
}

export function isLastStep(state: FlowState): boolean {
  return state.current === STEPS.length - 1;
}

/** "2 of 5" for the title band. */
export function stepCount(state: FlowState): string {
  return `${state.current + 1} of ${STEPS.length}`;
}
