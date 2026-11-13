import { describe, expect, it } from "vitest";
import type { JobEvent } from "./bindings";
import { createSetupMock, STEP_WEIGHT, UPDATE_STEPS } from "./mock";

/** Run the mock's update to the end and collect what it reported. */
async function runUpdate(failAt: string | null = null) {
  const mock = createSetupMock({ mode: "update", existing: false, failAt, tickMs: 0 });
  const events: JobEvent[] = [];
  const finished = new Promise<void>((resolve) => {
    mock.subscribe((event) => {
      events.push(event);
      if (event.type === "finished" || event.type === "failed") resolve();
    });
  });
  const steps = await mock.commands.update();
  await finished;
  return { mock, steps, events };
}

describe("setup mock: an update Cilica handed over", () => {
  it("names the version it updates to and the one installed", async () => {
    const mock = createSetupMock({ mode: "update", existing: false, failAt: null, tickMs: 0 });
    const info = await mock.commands.getSetupInfo();
    expect(info.mode).toBe("update");
    expect(info.existing?.version).not.toBe(info.version);
  });

  it("waits for Cilica, then installs, and finishes", async () => {
    const { steps, events } = await runUpdate();
    expect(steps).toEqual(UPDATE_STEPS);
    expect(steps[0]).toBe("close");
    const done = events.flatMap((e) => (e.type === "step_done" ? [e.step] : []));
    expect(done).toEqual(UPDATE_STEPS);
    expect(events.at(-1)).toEqual({ type: "finished" });
  });

  it("stops at a failing step", async () => {
    const { events } = await runUpdate("copy");
    expect(events.at(-1)?.type).toBe("failed");
  });

  it("is refused outside update mode", async () => {
    const mock = createSetupMock({ mode: "install", existing: true, failAt: null, tickMs: 0 });
    await expect(mock.commands.update()).rejects.toMatchObject({ code: "conflict" });
  });

  it("weighs every step", () => {
    for (const step of UPDATE_STEPS) expect(STEP_WEIGHT[step]).toBeGreaterThan(0);
  });
});
