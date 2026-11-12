import { describe, expect, it } from "vitest";
import {
  type FlowState,
  flowReducer,
  initialFlow,
  isLastStep,
  STEPS,
  stepCount,
  stepStatus,
} from "./flow";

const run = (...actions: Parameters<typeof flowReducer>[1][]): FlowState =>
  actions.reduce(flowReducer, initialFlow);

describe("setup flow", () => {
  it("opens on the welcome screen, then the first step", () => {
    expect(initialFlow.phase).toBe("welcome");
    const started = run({ type: "start" });
    expect(started.phase).toBe("steps");
    expect(stepStatus(started, 0)).toBe("active");
    expect(stepStatus(started, 1)).toBe("upcoming");
    expect(stepCount(started)).toBe(`1 of ${STEPS.length}`);
  });

  it("collapses finished steps and remembers which were skipped", () => {
    const state = run(
      { type: "start" },
      { type: "next", skipped: false },
      { type: "next", skipped: true },
    );
    expect(state.current).toBe(2);
    expect(stepStatus(state, 0)).toBe("done");
    expect(stepStatus(state, 1)).toBe("done");
    expect(state.skipped).toEqual({ speech: true });
  });

  it("clears a skip once the step is done properly", () => {
    const state = run(
      { type: "start" },
      { type: "next", skipped: true },
      { type: "reopen", index: 0 },
      { type: "next", skipped: false },
    );
    expect(state.skipped.permissions).toBeUndefined();
    expect(state.current).toBe(1);
  });

  it("reopens only steps already reached", () => {
    const state = run({ type: "start" }, { type: "next", skipped: false });
    expect(flowReducer(state, { type: "reopen", index: 3 })).toBe(state);
    expect(flowReducer(state, { type: "reopen", index: 0 }).current).toBe(0);
  });

  it("ends on the last step: there is no page after it", () => {
    let state = run({ type: "start" });
    for (let i = 0; i < STEPS.length + 2; i++)
      state = flowReducer(state, { type: "next", skipped: false });
    expect(state.current).toBe(STEPS.length - 1);
    expect(isLastStep(state)).toBe(true);
    expect(STEPS.at(-1)).toBe("look");
  });

  it("goes back to the welcome screen without losing progress", () => {
    const state = run({ type: "start" }, { type: "next", skipped: false }, { type: "welcome" });
    expect(state.phase).toBe("welcome");
    expect(state.current).toBe(1);
  });
});
