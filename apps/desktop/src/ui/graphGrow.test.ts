import { APPEAR_SETTLE, appearOpacity, appearScale, GraphGrowth } from "./graphGrow";

describe("graph grow replay", () => {
  it("shows the steps in order and ends", () => {
    const g = new GraphGrowth([2, 0, 1, 1]);
    expect(g.steps).toBe(3);
    // The lead-in shows nothing.
    expect(g.advance(0.1)).toEqual([0, 0]);
    let shown = 0;
    let frames = 0;
    while (!g.done && frames < 10_000) {
      const [from, to] = g.advance(1 / 60);
      expect(from).toBe(shown);
      expect(to).toBeGreaterThanOrEqual(from);
      shown = to;
      frames++;
    }
    expect(g.done).toBe(true);
    expect(g.shown).toBe(3);
    // Short graphs still take a moment, never longer than the cap.
    expect(frames / 60).toBeGreaterThan(2);
    expect(frames / 60).toBeLessThan(13);
  });

  it("caps a long replay and shows several steps a frame", () => {
    const g = new GraphGrowth(Array.from({ length: 5000 }, (_, i) => i));
    let seconds = 0;
    while (!g.done) {
      g.advance(1 / 60);
      seconds += 1 / 60;
    }
    expect(seconds).toBeLessThan(12.5);
  });

  it("shows everything when stopped", () => {
    const g = new GraphGrowth([0, 1, 2]);
    g.advance(1);
    expect(g.finish()[1]).toBe(3);
    expect(g.done).toBe(true);
  });

  it("springs a node in with a little overshoot", () => {
    expect(appearScale(0)).toBe(0);
    expect(appearScale(APPEAR_SETTLE)).toBe(1);
    let peak = 0;
    for (let t = 0; t < APPEAR_SETTLE; t += 0.005) peak = Math.max(peak, appearScale(t));
    expect(peak).toBeGreaterThan(1);
    expect(peak).toBeLessThan(1.2);
    expect(Math.abs(appearScale(APPEAR_SETTLE - 0.001) - 1)).toBeLessThan(0.02);
    expect(appearOpacity(0)).toBe(0);
    expect(appearOpacity(1)).toBe(1);
  });
});
