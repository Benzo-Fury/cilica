import { describe, expect, it } from "vitest";
import { HoverFade } from "./graphHover";

const FRAME = 1 / 60;

function run(fade: HoverFade, hover: number, seconds: number) {
  for (let t = 0; t < seconds; t += FRAME) fade.step(hover, FRAME);
}

describe("HoverFade", () => {
  it("eases a hovered node in quickly", () => {
    const fade = new HoverFade();
    expect(fade.step(0, FRAME)).toBe(true);
    const first = fade.amountOf(0);
    expect(first).toBeGreaterThan(0);
    expect(first).toBeLessThan(1);
    run(fade, 0, 0.15);
    expect(fade.amountOf(0)).toBe(1);
    expect(fade.step(0, FRAME)).toBe(false);
  });

  it("crossfades from one node to the next", () => {
    const fade = new HoverFade();
    run(fade, 0, 0.25);
    fade.step(1, FRAME);
    // Midway: the old glow is fading out while the new one fades in, and the graph stays dimmed.
    expect(fade.amountOf(0)).toBeGreaterThan(0);
    expect(fade.amountOf(0)).toBeLessThan(1);
    expect(fade.amountOf(1)).toBeGreaterThan(0);
    expect(fade.dim).toBe(1);
    run(fade, 1, 0.25);
    expect(fade.amountOf(0)).toBe(0);
    expect(fade.amountOf(1)).toBe(1);
    expect(fade.glows).toHaveLength(1);
  });

  it("fades everything out when the pointer leaves", () => {
    const fade = new HoverFade();
    run(fade, 2, 0.25);
    run(fade, -1, 0.25);
    expect(fade.glows).toHaveLength(0);
    expect(fade.dim).toBe(0);
  });

  it("lights a glow's node and neighbours by its amount", () => {
    const fade = new HoverFade();
    run(fade, 0, 0.25);
    const lit = fade.litness([[1], [0, 2], [1]], new Float32Array(3));
    expect(Array.from(lit)).toEqual([1, 1, 0]);
  });
});
