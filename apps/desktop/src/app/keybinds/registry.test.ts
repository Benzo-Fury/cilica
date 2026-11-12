import {
  ACTIONS,
  type ActionDef,
  effectiveKeys,
  eventToAccelerator,
  findConflict,
  formatAccelerator,
  getAction,
  isValidBinding,
  listConflicts,
  matchAction,
  normalizeAccelerator,
  rebind,
  resetBinding,
} from "./registry";

const key = (
  code: string,
  mods: Partial<Record<"meta" | "ctrl" | "alt" | "shift", boolean>> = {},
) => ({
  code,
  key: code.replace(/^Key/, "").toLowerCase(),
  metaKey: !!mods.meta,
  ctrlKey: !!mods.ctrl,
  altKey: !!mods.alt,
  shiftKey: !!mods.shift,
});

describe("accelerators", () => {
  it("normalises spelling and modifier order", () => {
    expect(normalizeAccelerator("shift+mod+r")).toBe("Mod+Shift+R");
    expect(normalizeAccelerator("CmdOrCtrl+,")).toBe("Mod+,");
    expect(normalizeAccelerator("Mod+backspace")).toBe("Mod+Backspace");
    expect(normalizeAccelerator("Mod+\\")).toBe("Mod+\\");
    expect(normalizeAccelerator("Hyper+K")).toBeNull();
  });

  it("maps ⌘ to Mod on macOS and Ctrl to Mod elsewhere", () => {
    expect(eventToAccelerator(key("KeyK", { meta: true }), true)).toBe("Mod+K");
    expect(eventToAccelerator(key("KeyK", { ctrl: true }), true)).toBe("Ctrl+K");
    expect(eventToAccelerator(key("KeyK", { ctrl: true }), false)).toBe("Mod+K");
    expect(eventToAccelerator(key("KeyR", { meta: true, shift: true }), true)).toBe("Mod+Shift+R");
    expect(eventToAccelerator(key("Backslash", { meta: true }), true)).toBe("Mod+\\");
    expect(eventToAccelerator({ ...key("ShiftLeft"), key: "Shift" }, true)).toBeNull();
  });

  it("uses the physical key so layouts do not matter", () => {
    // Shift+/ produces "?" on a US layout but the accelerator stays "/".
    expect(
      eventToAccelerator({ ...key("Slash", { meta: true, shift: true }), key: "?" }, true),
    ).toBe("Mod+Shift+/");
  });

  it("formats keycaps per platform", () => {
    expect(formatAccelerator("Mod+Shift+R", true)).toEqual(["⌘", "⇧", "R"]);
    expect(formatAccelerator("Mod+Shift+R", false)).toEqual(["Ctrl", "Shift", "R"]);
    expect(formatAccelerator("Mod+Backspace", true)).toEqual(["⌘", "⌫"]);
    expect(formatAccelerator("Escape", false)).toEqual(["Esc"]);
    expect(formatAccelerator(null, true)).toEqual([]);
  });

  it("only accepts shortcuts that cannot fire while typing", () => {
    expect(isValidBinding("Mod+K")).toBe(true);
    expect(isValidBinding("Alt+Shift+K")).toBe(true);
    expect(isValidBinding("F5")).toBe(true);
    expect(isValidBinding("K")).toBe(false);
    expect(isValidBinding("Shift+K")).toBe(false);
  });
});

describe("registry", () => {
  it("has unique ids and no conflicts by default", () => {
    const ids = ACTIONS.map((a) => a.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(listConflicts({})).toEqual([]);
  });

  it("matches the spec defaults", () => {
    expect(matchAction("Mod+K", {})?.id).toBe("palette.open");
    expect(matchAction("Mod+Shift+N", {})?.id).toBe("category.new");
    expect(matchAction("Mod+Backspace", {})?.id).toBe("note.trash");
    expect(matchAction("Mod+/", {})?.id).toBe("cheatsheet.open");
    expect(matchAction("Mod+Shift+K", {})).toBeUndefined();
  });

  it("applies overrides and lets an empty override unbind", () => {
    const overrides = { "palette.open": "Mod+Shift+P" };
    expect(matchAction("Mod+Shift+P", overrides)?.id).toBe("palette.open");
    expect(matchAction("Mod+K", overrides)).toBeUndefined();
    const palette = getAction("palette.open") as ActionDef;
    expect(effectiveKeys(palette, { "palette.open": "" })).toBeNull();
  });

  it("ignores overrides for actions that cannot be rebound", () => {
    const close = getAction("layer.close") as ActionDef;
    expect(effectiveKeys(close, { "layer.close": "Mod+W" })).toBe("Escape");
  });

  it("detects conflicts", () => {
    expect(findConflict("palette.open", "Mod+Shift+N", {})?.id).toBe("category.new");
    expect(findConflict("palette.open", "Mod+K", {})).toBeUndefined();
    expect(listConflicts({ "note.new": "Mod+K" })).toEqual([["palette.open", "note.new"]]);
  });

  it("rebinds, replaces and resets", () => {
    let overrides: Record<string, string> = rebind({}, "palette.open", "Mod+Shift+N", {
      replace: true,
    });
    expect(overrides).toEqual({ "palette.open": "Mod+Shift+N", "category.new": "" });
    expect(listConflicts(overrides)).toEqual([]);
    overrides = rebind(overrides, "palette.open", "Mod+K");
    expect(overrides).toEqual({ "category.new": "" });
    expect(resetBinding(overrides, "category.new")).toEqual({});
    expect(resetBinding({ a: "Mod+1", b: "Mod+2" })).toEqual({});
  });
});
