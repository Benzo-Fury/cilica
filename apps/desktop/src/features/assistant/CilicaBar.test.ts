import { describe, expect, it } from "vitest";
import { openSize } from "./CilicaBar";

describe("openSize", () => {
  it("hugs its content", () => {
    expect(openSize({ width: 900, height: 600 }, 132)).toEqual({ width: 560, height: 132 });
    expect(openSize({ width: 900, height: 700 }, 240)).toEqual({ width: 560, height: 240 });
  });

  it("stops growing at the conversation cap", () => {
    expect(openSize({ width: 900, height: 900 }, 2000)).toEqual({ width: 560, height: 560 });
  });

  it("never asks for more room than the page has", () => {
    expect(openSize({ width: 420, height: 300 }, 400)).toEqual({ width: 388, height: 300 });
    expect(openSize({ width: 420, height: 90 }, 132).height).toBe(90);
  });

  it("is never smaller than the resting pill", () => {
    expect(openSize({ width: 200, height: 20 }, 0)).toEqual({ width: 310, height: 44 });
  });
});
