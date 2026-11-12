import { ForceSim } from "./graphSim";

function settle(sim: ForceSim, max = 2000) {
  let ticks = 0;
  while (!sim.settled && ticks < max) {
    sim.tick();
    ticks++;
  }
  return ticks;
}

const OPTIONS = { linkDistance: 50, charge: -140, gravity: 0.05 };

describe("force layout", () => {
  it("settles and stops", () => {
    const sim = new ForceSim(
      3,
      [
        { source: 0, target: 1 },
        { source: 1, target: 2 },
      ],
      OPTIONS,
    );
    const ticks = settle(sim);
    expect(sim.settled).toBe(true);
    expect(ticks).toBeLessThan(600);
  });

  it("keeps linked nodes closer than unlinked ones", () => {
    const sim = new ForceSim(4, [{ source: 0, target: 1 }], OPTIONS);
    settle(sim);
    const d = (a: number, b: number) =>
      Math.hypot((sim.x[a] ?? 0) - (sim.x[b] ?? 0), (sim.y[a] ?? 0) - (sim.y[b] ?? 0));
    expect(d(0, 1)).toBeLessThan(d(2, 3));
    expect(Number.isFinite(d(0, 1))).toBe(true);
  });

  it("lays out a large graph with Barnes–Hut fast enough", () => {
    const n = 3000;
    const edges = Array.from({ length: n * 2 }, (_, i) => ({
      source: i % n,
      target: (i * 7 + 1) % n,
    }));
    const sim = new ForceSim(n, edges, OPTIONS);
    const start = performance.now();
    for (let i = 0; i < 20; i++) sim.tick();
    const perTick = (performance.now() - start) / 20;
    // Generous for CI; on a laptop it is a few milliseconds.
    expect(perTick).toBeLessThan(60);
    for (let i = 0; i < n; i++) {
      expect(Number.isFinite(sim.x[i])).toBe(true);
      expect(Number.isFinite(sim.y[i])).toBe(true);
    }
    // Nodes spread out instead of collapsing to a point.
    let maxR = 0;
    for (let i = 0; i < n; i++) maxR = Math.max(maxR, Math.hypot(sim.x[i] ?? 0, sim.y[i] ?? 0));
    expect(maxR).toBeGreaterThan(100);
  });

  it("starts nodes where they were", () => {
    const sim = new ForceSim(2, [], OPTIONS, (i) => (i === 0 ? { x: 400, y: -20 } : null));
    expect(sim.x[0]).toBe(400);
    expect(sim.y[0]).toBe(-20);
  });

  it("holds a pinned node still", () => {
    const sim = new ForceSim(2, [{ source: 0, target: 1 }], OPTIONS);
    sim.pinned[0] = 1;
    sim.x[0] = 1000;
    sim.y[0] = 1000;
    for (let i = 0; i < 50; i++) sim.tick();
    expect(sim.x[0]).toBe(1000);
    expect(sim.y[0]).toBe(1000);
  });
});
