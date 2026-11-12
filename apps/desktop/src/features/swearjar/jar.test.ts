import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  CATCH_UP_COINS,
  dropCoins,
  light,
  reconcile,
  registerJar,
  resetJar,
  useJar,
  WASH_MS,
} from "./jar";

describe("the jar on screen", () => {
  beforeEach(() => {
    resetJar();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("starts from the backend's count without dropping anything", () => {
    reconcile(22, { animate: true, away: false });
    expect(useJar.getState()).toMatchObject({ seen: 22, shown: false });
  });

  it("counts quietly when no jar is on screen", () => {
    reconcile(5, { animate: true, away: false });
    dropCoins([null, null]);
    expect(useJar.getState().seen).toBe(7);
  });

  it("flies each coin, ticks the label on landing, then slides away", async () => {
    const fly = vi.fn(() => Promise.resolve());
    registerJar({ fly });
    reconcile(10, { animate: true, away: false });
    dropCoins([new DOMRect(1, 1, 10, 10), null]);
    expect(useJar.getState()).toMatchObject({ shown: true, owed: 2, seen: 10 });
    await vi.advanceTimersByTimeAsync(200);
    expect(fly).toHaveBeenCalledTimes(2);
    expect(useJar.getState()).toMatchObject({ seen: 12, owed: 0, streak: 2 });
    await vi.advanceTimersByTimeAsync(2000);
    expect(useJar.getState()).toMatchObject({ shown: false, streak: 0 });
  });

  it("catches up on coins said while away, five at most in the air", async () => {
    const fly = vi.fn(() => Promise.resolve());
    registerJar({ fly });
    reconcile(3, { animate: true, away: false });
    reconcile(12, { animate: true, away: true });
    expect(useJar.getState()).toMatchObject({ away: true, owed: 9 });
    await vi.advanceTimersByTimeAsync(1000);
    expect(fly).toHaveBeenCalledTimes(CATCH_UP_COINS);
    expect(useJar.getState()).toMatchObject({ seen: 12, streak: 9 });
  });

  it("does not count coins twice while they are in the air", () => {
    registerJar({ fly: () => new Promise(() => undefined) });
    reconcile(4, { animate: true, away: false });
    dropCoins([null]);
    reconcile(5, { animate: true, away: false });
    expect(useJar.getState()).toMatchObject({ seen: 4, owed: 1 });
  });

  it("resets the label when the count drops", () => {
    reconcile(30, { animate: true, away: false });
    reconcile(0, { animate: true, away: false });
    expect(useJar.getState().seen).toBe(0);
  });

  it("washes a line's swears for a moment, then lets go", () => {
    light("seg_1");
    light("seg_1");
    expect([...useJar.getState().lit]).toEqual(["seg_1"]);
    vi.advanceTimersByTime(WASH_MS);
    expect(useJar.getState().lit.size).toBe(0);
  });
});
