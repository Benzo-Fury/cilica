import { describe, expect, it } from "vitest";
import { initialJob, isBusy, type JobState, jobReducer, sunHeight } from "./flow";

const started = jobReducer(jobReducer(initialJob, { type: "starting" }), {
  type: "started",
  steps: ["check", "copy", "register"],
});

describe("installer flow", () => {
  it("starts on the first step", () => {
    expect(started.screen).toBe("working");
    expect(started.current).toBe("check");
    expect(isBusy(started)).toBe(true);
  });

  it("moves on as steps finish and never moves the bar back", () => {
    let state: JobState = jobReducer(started, {
      type: "event",
      event: { type: "step", step: "check", progress: 0.3 },
    });
    state = jobReducer(state, {
      type: "event",
      event: { type: "step_done", step: "check", progress: 0.4 },
    });
    expect(state.done).toEqual(["check"]);
    expect(state.current).toBe("copy");
    state = jobReducer(state, {
      type: "event",
      event: { type: "step", step: "copy", progress: 0.2 },
    });
    expect(state.progress).toBe(0.4);
    state = jobReducer(state, {
      type: "event",
      event: { type: "step_done", step: "check", progress: 0.4 },
    });
    expect(state.done).toEqual(["check"]);
  });

  it("keeps steps Setup reported before it answered", () => {
    let state = jobReducer(initialJob, { type: "starting" });
    state = jobReducer(state, {
      type: "event",
      event: { type: "step_done", step: "check", progress: 0.1 },
    });
    state = jobReducer(state, { type: "started", steps: ["check", "copy"] });
    expect(state.done).toEqual(["check"]);
    expect(state.progress).toBe(0.1);
    expect(state.steps).toEqual(["check", "copy"]);
  });

  it("reads unknown progress as nothing new", () => {
    const state = jobReducer(started, {
      type: "event",
      event: { type: "step", step: "check", progress: null },
    });
    expect(state.progress).toBe(0);
  });

  it("finishes on the done screen", () => {
    const state = jobReducer(started, { type: "event", event: { type: "finished" } });
    expect(state.screen).toBe("done");
    expect(state.progress).toBe(1);
    expect(isBusy(state)).toBe(false);
  });

  it("keeps a failure on screen and ignores later events", () => {
    const error = { code: "internal" as const, message: "It stopped. Try again." };
    let state = jobReducer(started, { type: "event", event: { type: "failed", error } });
    expect(state.error).toEqual(error);
    expect(isBusy(state)).toBe(false);
    state = jobReducer(state, { type: "event", event: { type: "finished" } });
    expect(state.screen).toBe("working");
    expect(jobReducer(state, { type: "retry" })).toEqual(initialJob);
  });

  it("shows a refused start as a failure", () => {
    const error = { code: "conflict" as const, message: "Setup is already working." };
    const state = jobReducer(initialJob, { type: "refused", error });
    expect(state.screen).toBe("working");
    expect(state.error).toEqual(error);
  });

  it("raises the sun to install or update and sets it to remove", () => {
    expect(sunHeight("install", initialJob)).toBeCloseTo(0.2);
    expect(sunHeight("update", initialJob)).toBeCloseTo(0.2);
    expect(sunHeight("update", { ...started, screen: "done" })).toBeCloseTo(1.6);
    expect(sunHeight("install", { ...started, progress: 0.5 })).toBeCloseTo(0.9);
    expect(sunHeight("install", { ...started, screen: "done" })).toBeCloseTo(1.6);
    expect(sunHeight("uninstall", initialJob)).toBeCloseTo(1.6);
    expect(sunHeight("uninstall", { ...started, progress: 1 })).toBeCloseTo(0.2);
    expect(sunHeight("uninstall", { ...started, screen: "done" })).toBeCloseTo(0.2);
  });
});
