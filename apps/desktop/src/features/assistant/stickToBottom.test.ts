import { describe, expect, it } from "vitest";
import { NEAR_BOTTOM, nextFollowing } from "./stickToBottom";

describe("nextFollowing", () => {
  it("stops when you scroll up", () => {
    expect(nextFollowing(true, { top: 100, previousTop: 300, distance: 400 })).toBe(false);
  });

  it("keeps following while a smooth scroll is still on its way down", () => {
    expect(nextFollowing(true, { top: 300, previousTop: 100, distance: 400 })).toBe(true);
  });

  it("keeps following when content grows below without a scroll", () => {
    expect(nextFollowing(true, { top: 300, previousTop: 300, distance: 900 })).toBe(true);
  });

  it("starts again once you are back near the bottom", () => {
    expect(nextFollowing(false, { top: 500, previousTop: 400, distance: NEAR_BOTTOM - 1 })).toBe(
      true,
    );
  });

  it("stays off while you scroll down but are not at the bottom yet", () => {
    expect(nextFollowing(false, { top: 400, previousTop: 300, distance: 400 })).toBe(false);
  });
});
