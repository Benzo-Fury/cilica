import { describe, expect, it } from "vitest";
import { CANVAS_MIN, maxPanelWidth, panelToClose } from "./panels";

describe("panel fitting", () => {
  it("closes the panel opened earlier when both would squeeze the canvas", () => {
    expect(panelToClose(880, 248, 380, "assistant")).toBe("sidebar");
    expect(panelToClose(880, 248, 380, "sidebar")).toBe("assistant");
  });

  it("leaves things alone when they fit or only one panel is open", () => {
    expect(panelToClose(1200, 248, 380, "assistant")).toBeNull();
    expect(panelToClose(880, null, 520, "assistant")).toBeNull();
    expect(panelToClose(880, 360, null, "sidebar")).toBeNull();
  });

  it("stops a drag short of the canvas minimum, never below the panel minimum", () => {
    expect(maxPanelWidth(520, 320, 1100, 248)).toBe(1100 - 248 - CANVAS_MIN);
    expect(maxPanelWidth(520, 320, 2000, 248)).toBe(520);
    expect(maxPanelWidth(520, 320, 880, 248)).toBe(320);
  });
});
