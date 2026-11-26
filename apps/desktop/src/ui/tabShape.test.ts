import { describe, expect, it } from "vitest";
import { TAB_RADII, tabGround, tabOutline } from "./tabShape";

/** Every number in a path, in order. */
const numbers = (path: string) => (path.match(/-?\d+(\.\d+)?/g) ?? []).map(Number);

describe("tabOutline", () => {
  it("rounds both page corners when no tab is selected", () => {
    const path = tabOutline({ width: 400, left: 0, right: 0, depth: 0 });
    expect(path.match(/A/g)).toHaveLength(2);
    expect(path.startsWith("M0 0")).toBe(true);
    expect(path.endsWith("L400 0")).toBe(true);
  });

  it("turns the page down into a tab in the middle through two fillets", () => {
    const path = tabOutline({ width: 600, left: 200, right: 360, depth: 36 });
    // Page corners, two fillets and the tab's two bottom corners.
    expect(path.match(/A/g)).toHaveLength(6);
    expect(path).toContain(`L200 ${TAB_RADII.strip + 36 - TAB_RADII.corner}`);
  });

  it("runs a tab flush against the left edge straight down from the page", () => {
    const path = tabOutline({ width: 600, left: 0, right: 160, depth: 36 });
    // No page corner and no fillet on the left: those become straight lines.
    expect(path.match(/A/g)).toHaveLength(4);
    expect(path).toContain(`L0 ${TAB_RADII.strip}`);
  });

  it("stays inside the bar at every point of a move", () => {
    for (let left = -20; left <= 620; left += 7) {
      for (const depth of [0, 3, 12, 36]) {
        const path = tabOutline({ width: 600, left, right: left + 140, depth });
        const xs = numbers(path).filter((_, i) => i % 2 === 0);
        expect(Math.min(...xs)).toBeGreaterThanOrEqual(0);
        expect(Number.isFinite(Math.max(...numbers(path)))).toBe(true);
      }
    }
  });
});

describe("tabGround", () => {
  it("is the whole bar with the page-and-tab shape cut out", () => {
    const path = tabGround({ width: 300, left: 40, right: 140, depth: 30 }, 44);
    expect(
      path.startsWith(`M0 0 L300 0 L300 ${TAB_RADII.strip + 44} L0 ${TAB_RADII.strip + 44} Z`),
    ).toBe(true);
    expect(path.trim().endsWith("Z")).toBe(true);
  });
});
