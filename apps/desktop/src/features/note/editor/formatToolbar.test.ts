import { Editor } from "@tiptap/core";
import { afterEach, describe, expect, it } from "vitest";
import { noteExtensions } from "./extensions";
import { hasFormattableSelection, normalizeHref, placeToolbar } from "./formatToolbar";

const viewport = { width: 1000, height: 800 };
const bar = { width: 200, height: 36 };

describe("placeToolbar", () => {
  it("centres the bar above the selection", () => {
    const p = placeToolbar({ left: 400, right: 600, top: 300, bottom: 320 }, bar, viewport);
    expect(p).toEqual({ left: 400, top: 256, side: "above" });
  });

  it("flips below when there is no room above", () => {
    const p = placeToolbar({ left: 400, right: 600, top: 20, bottom: 40 }, bar, viewport);
    expect(p).toEqual({ left: 400, top: 48, side: "below" });
  });

  it("stays inside the left and right edges", () => {
    expect(placeToolbar({ left: 0, right: 10, top: 300, bottom: 320 }, bar, viewport).left).toBe(8);
    expect(
      placeToolbar({ left: 990, right: 1000, top: 300, bottom: 320 }, bar, viewport).left,
    ).toBe(792);
  });

  it("pins to the top when the selection fills the window", () => {
    const p = placeToolbar({ left: 400, right: 600, top: 10, bottom: 790 }, bar, viewport);
    expect(p).toEqual({ left: 400, top: 8, side: "above" });
  });
});

describe("normalizeHref", () => {
  it.each([
    ["", ""],
    ["   ", ""],
    ["cilica.app", "https://cilica.app"],
    ["https://cilica.app/a?b", "https://cilica.app/a?b"],
    ["localhost:3000", "https://localhost:3000"],
    ["ada@example.com", "mailto:ada@example.com"],
    ["mailto:ada@example.com", "mailto:ada@example.com"],
    ["/notes/1", "/notes/1"],
    ["#heading", "#heading"],
  ])("%j → %j", (input, href) => {
    expect(normalizeHref(input)).toBe(href);
  });
});

describe("hasFormattableSelection", () => {
  let editor: Editor | null = null;
  afterEach(() => editor?.destroy());

  const make = (markdown: string) => {
    editor = new Editor({
      extensions: noteExtensions(),
      content: markdown,
      contentType: "markdown",
    });
    return editor;
  };

  it("is true for selected text", () => {
    const e = make("Hello world");
    e.commands.setTextSelection({ from: 1, to: 6 });
    expect(hasFormattableSelection(e.state, true)).toBe(true);
  });

  it("is false for a caret, a read-only editor or whitespace", () => {
    const e = make("Hello world");
    e.commands.setTextSelection(3);
    expect(hasFormattableSelection(e.state, true)).toBe(false);
    e.commands.setTextSelection({ from: 1, to: 6 });
    expect(hasFormattableSelection(e.state, false)).toBe(false);
    e.commands.setTextSelection({ from: 6, to: 7 });
    expect(hasFormattableSelection(e.state, true)).toBe(false);
  });

  it("is false inside a code block", () => {
    const e = make("```\nconst a = 1;\n```");
    e.commands.setTextSelection({ from: 2, to: 7 });
    expect(hasFormattableSelection(e.state, true)).toBe(false);
  });
});
