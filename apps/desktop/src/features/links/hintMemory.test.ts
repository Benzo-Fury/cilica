import { describe, expect, it } from "vitest";
import { type HintOutcome, isQuiet, isTrusted, remember, type TitleMemory } from "./hintMemory";

const play = (...outcomes: HintOutcome[]) =>
  outcomes.reduce<TitleMemory | undefined>((m, o) => remember(m, o), undefined);

describe("remembering how you answer link hints", () => {
  it("goes quiet after three ignores with no link", () => {
    expect(isQuiet(play("passed", "passed"))).toBe(false);
    expect(isQuiet(play("passed", "passed", "passed"))).toBe(true);
  });

  it("counts Esc as a firmer no", () => {
    expect(isQuiet(play("refused"))).toBe(false);
    expect(isQuiet(play("refused", "passed"))).toBe(true);
  });

  it("starts over, and trusts the title, once you link it", () => {
    const m = play("passed", "passed", "taken", "passed", "passed");
    expect(isQuiet(m)).toBe(false);
    expect(isTrusted(m)).toBe(true);
    expect(isTrusted(play("passed"))).toBe(false);
    expect(isQuiet(undefined)).toBe(false);
  });
});
