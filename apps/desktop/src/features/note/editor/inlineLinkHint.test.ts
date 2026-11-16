import { Editor } from "@tiptap/core";
import { afterEach, describe, expect, it } from "vitest";
import { buildTitleIndex, matchTitleBefore } from "@/features/links/inlineLink";
import { linkKey } from "@/lib/links";
import { noteExtensions } from "./extensions";
import { type InlineLinkHint, type InlineLinkOutcome, inlineLinkKey } from "./inlineLinkHint";

let editor: Editor | null = null;
afterEach(() => {
  editor?.destroy();
  editor = null;
});

const ALIASES: Record<string, string[]> = { Mitochondria: ["powerhouse of the cell"] };
const TITLES = ["Cytoplasm", "Cell", "Cell membrane", "Mitochondria", "Biology 101", "Lisbon"];

function mount(markdown = "", { self = "Biology 101" } = {}) {
  const index = buildTitleIndex(
    new Map(
      TITLES.map((t) => [linkKey(t), { id: `id-${linkKey(t)}`, title: t, aliases: ALIASES[t] }]),
    ),
  );
  const shown: (InlineLinkHint | null)[] = [];
  const outcomes: string[] = [];
  const element = document.createElement("div");
  document.body.append(element);
  const e = new Editor({
    element,
    extensions: noteExtensions({
      linkHint: {
        match: (before, isLinked) =>
          matchTitleBefore(before, index, {
            skip: (key) => key === linkKey(self) || isLinked(key),
          }),
        handlers: () => ({
          onChange: (hint) => shown.push(hint),
          onOutcome: (hint, outcome: InlineLinkOutcome) =>
            outcomes.push(`${hint.title}:${outcome}`),
        }),
      },
    }),
    content: markdown,
    contentType: "markdown",
  });
  editor = e;
  // Caret at the end of the last paragraph.
  e.commands.setTextSelection(e.state.doc.content.size - 1);
  const view = e.view;

  /** Type like a person: one key at a time, through the same hook the browser uses. */
  const type = (text: string) => {
    for (const ch of text) {
      if (ch === "\n") {
        press("Enter");
        continue;
      }
      const { from, to } = view.state.selection;
      const handled = view.someProp("handleTextInput", (f) =>
        f(view, from, to, ch, () => view.state.tr),
      );
      if (!handled) view.dispatch(view.state.tr.insertText(ch, from, to));
    }
  };
  const press = (key: string) => {
    const event = new KeyboardEvent("keydown", { key, cancelable: true });
    const handled = view.someProp("handleKeyDown", (f) => f(view, event));
    if (!handled && key === "Enter") e.commands.splitBlock();
    return Boolean(handled);
  };
  const hint = () => inlineLinkKey.getState(view.state)?.hint ?? null;
  return { e, type, press, hint, shown, outcomes, md: () => e.getMarkdown().trim() };
}

describe("linking a title as you type it", () => {
  it("offers the note the moment the word is finished", () => {
    const { type, hint } = mount();
    type("The cytoplas");
    expect(hint()).toBeNull();
    type("m");
    expect(hint()).toBeNull(); // not finished yet: you might be typing "cytoplasmic"
    type(" ");
    expect(hint()).toMatchObject({ title: "Cytoplasm", typed: "cytoplasm" });
  });

  it("Tab links it, keeping the words as typed, and typing goes on as normal", () => {
    const { type, press, hint, md } = mount();
    type("The cytoplasm ");
    expect(press("Tab")).toBe(true);
    expect(hint()).toBeNull();
    type("is a gel.");
    expect(md()).toBe("The [[cytoplasm]] is a gel.");
  });

  it("goes away when you keep typing, and the text is untouched", () => {
    const { type, press, hint, md } = mount();
    type("The cytoplasm ");
    expect(hint()).not.toBeNull();
    type("i");
    expect(hint()).toBeNull();
    type("s full of water. ");
    // Tab is not taken once the hint has gone.
    expect(press("Tab")).toBe(false);
    expect(md()).toBe("The cytoplasm is full of water.");
  });

  it("also shows after punctuation and Enter", () => {
    const comma = mount();
    comma.type("Inside the cytoplasm,");
    expect(comma.hint()?.title).toBe("Cytoplasm");
    comma.e.destroy();

    const enter = mount();
    enter.type("Mitochondria\n");
    expect(enter.hint()?.title).toBe("Mitochondria");
    expect(enter.press("Tab")).toBe(true);
    expect(enter.md()).toBe("[[Mitochondria]]");
  });

  it("also works when a word lands in one go (autocorrect, text replacement, dictation)", () => {
    const { e, hint, press, md } = mount();
    const view = e.view;
    const typeChunk = (text: string, replace = 0) => {
      const { from } = view.state.selection;
      const handled = view.someProp("handleTextInput", (f) =>
        f(view, from - replace, from, text, () => view.state.tr),
      );
      if (!handled) view.dispatch(view.state.tr.insertText(text, from - replace, from));
    };
    typeChunk("Notes on the ");
    typeChunk("mitochondira", 0);
    // Autocorrect swaps the misspelled word for the right one and the space, in one change.
    typeChunk("mitochondria ", "mitochondira".length);
    expect(hint()?.title).toBe("Mitochondria");
    press("Tab");
    expect(md()).toBe("Notes on the [[mitochondria]]");
  });

  it("follows a multi-word title as it grows", () => {
    const { type, press, hint, md } = mount();
    type("the cell ");
    expect(hint()?.title).toBe("Cell");
    type("membrane ");
    expect(hint()?.title).toBe("Cell membrane");
    press("Tab");
    expect(md()).toBe("the [[cell membrane]]");
  });

  it("offers each title once per note", () => {
    const { type, hint } = mount();
    type("cytoplasm and ");
    expect(hint()).toBeNull();
    type("more cytoplasm ");
    expect(hint()).toBeNull();
  });

  it("skips titles already linked, this note's own title, and code", () => {
    const linked = mount("We covered [[Cytoplasm]] already.\n\n");
    linked.type("The cytoplasm ");
    expect(linked.hint()).toBeNull();
    linked.e.destroy();

    const own = mount();
    own.type("Notes for biology 101 ");
    expect(own.hint()).toBeNull();
    own.e.destroy();

    const code = mount("```\n\n```");
    code.e.commands.setTextSelection(2);
    code.type("cytoplasm ");
    expect(code.hint()).toBeNull();
  });

  it("Esc, moving the caret, or a word you did not just type put it away or never show it", () => {
    const esc = mount();
    esc.type("cytoplasm ");
    expect(esc.press("Escape")).toBe(true);
    expect(esc.hint()).toBeNull();
    esc.e.destroy();

    const moved = mount();
    moved.type("mitochondria ");
    moved.e.commands.setTextSelection(1);
    expect(moved.hint()).toBeNull();
    moved.e.destroy();

    // A space added after a word already in the note is not "finishing" it.
    const old = mount("Cytoplasm");
    old.type(" ");
    expect(old.hint()).toBeNull();
  });

  it("tells the popup when it shows and goes", () => {
    const { type, shown } = mount();
    type("cytoplasm x");
    expect(shown.map((h) => h?.title ?? null)).toEqual(["Cytoplasm", null]);
  });

  it("links a plural to the note, showing the word as typed", () => {
    const { type, press, md } = mount();
    type("Both cytoplasms ");
    press("Tab");
    type("differ.");
    expect(md()).toBe("Both [[Cytoplasm|cytoplasms]] differ.");
  });

  it("links a possessive without its 's", () => {
    const straight = mount();
    straight.type("We loved Lisbon's ");
    expect(straight.hint()).toMatchObject({ title: "Lisbon", typed: "Lisbon" });
    straight.press("Tab");
    expect(straight.md()).toBe("We loved [[Lisbon]]'s");
    straight.e.destroy();

    // A curly apostrophe is not taken for a closing quote: no hint before the s.
    const curly = mount();
    curly.type("Lisbon’");
    expect(curly.hint()).toBeNull();
    curly.type("s ");
    curly.press("Tab");
    expect(curly.md()).toBe("[[Lisbon]]’s");
  });

  it("links an alias to its note, showing the alias", () => {
    const { type, press, hint, md } = mount();
    type("It is the powerhouse of the cell ");
    expect(hint()).toMatchObject({ title: "Mitochondria", typed: "powerhouse of the cell" });
    press("Tab");
    expect(md()).toBe("It is the [[Mitochondria|powerhouse of the cell]]");
  });

  it("reports how each hint ended", () => {
    const { e, type, press, outcomes } = mount();
    type("cytoplasm ");
    press("Tab");
    type("and mitochondria ");
    press("Escape");
    type("in the cell ");
    type("wall ");
    expect(outcomes).toEqual(["Cytoplasm:taken", "Mitochondria:refused", "Cell:passed"]);
    e.destroy();
  });

  it("does not count a title that grew into a longer one as ignored", () => {
    const { e, type, press, outcomes } = mount();
    type("the cell membrane ");
    press("Tab");
    expect(outcomes).toEqual(["Cell membrane:taken"]);
    // Typing past it in one go (dictation, text replacement) counts too.
    type("and mitochondria ");
    const view = e.view;
    const { from } = view.state.selection;
    view.someProp("handleTextInput", (f) => f(view, from, from, "are set. ", () => view.state.tr));
    view.dispatch(view.state.tr.insertText("are set. ", from));
    expect(outcomes).toEqual(["Cell membrane:taken", "Mitochondria:passed"]);
    // Typing past a hint is settled when the editor closes, too.
    type("in the cytoplasm x");
    e.destroy();
    expect(outcomes).toEqual(["Cell membrane:taken", "Mitochondria:passed", "Cytoplasm:passed"]);
  });

  it("in a list, Tab links while a hint shows and indents otherwise", () => {
    const linking = mount("- first\n- second");
    linking.type(" cytoplasm ");
    linking.press("Tab");
    expect(linking.md()).toBe("- first\n- second [[cytoplasm]]");
    linking.e.destroy();

    const indenting = mount("- first\n- second");
    indenting.type(" item");
    expect(indenting.hint()).toBeNull();
    indenting.press("Tab");
    expect(indenting.md()).toBe("- first\n  - second item");
  });
});
