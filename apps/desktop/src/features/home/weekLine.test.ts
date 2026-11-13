import { beforeEach, describe, expect, it } from "vitest";
import { dayKey, readWeekLine } from "./weekLine";

describe("week line cache", () => {
  beforeEach(() => localStorage.clear());

  it("keeps a line for its own day only", () => {
    const today = dayKey(new Date(2026, 10, 12, 9));
    localStorage.setItem(
      "cilica.weekLine",
      JSON.stringify({ day: today, line: "Seven meetings." }),
    );
    expect(readWeekLine(today)).toBe("Seven meetings.");
    expect(readWeekLine(dayKey(new Date(2026, 10, 13, 9)))).toBeNull();
  });

  it("ignores anything unreadable", () => {
    localStorage.setItem("cilica.weekLine", "not json");
    expect(readWeekLine(dayKey())).toBeNull();
  });

  it("changes at local midnight, not during the day", () => {
    expect(dayKey(new Date(2026, 10, 12, 0, 1))).toBe(dayKey(new Date(2026, 10, 12, 23, 59)));
  });
});
