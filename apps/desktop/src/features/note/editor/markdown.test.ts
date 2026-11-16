import { Editor } from "@tiptap/core";
import { createMockBackend } from "@/ipc/mock";
import { noteExtensions } from "./extensions";
import { findMatches } from "./findInNote";
import { normalizeMarkdown } from "./markdown";
import { looksLikeMarkdown } from "./markdownInput";
import type { SuggestHandlers } from "./wikiSuggest";

function roundTrip(markdown: string): string {
  const editor = new Editor({
    extensions: noteExtensions(),
    content: markdown,
    contentType: "markdown",
  });
  const out = editor.getMarkdown();
  editor.destroy();
  return normalizeMarkdown(out);
}

const EXACT = [
  "# Title\n\n## Decisions\n\nShip the **shorter** onboarding, *italic* and `code`.",
  "### Third level\n\n#### Fourth level",
  "- one\n- two\n  - nested\n\n1. first\n2. second",
  "- [ ] open task\n- [x] done task",
  "- [ ] parent\n  - [x] child",
  "> A quote\n> continues",
  "```ts\nconst a = 1;\n```",
  "A [link](https://example.com) here.",
  "Line one\n\n---\n\nAfter rule",
  "~~struck~~ text",
  "E = mc^2^ and H~2~O, then ~~struck~~.",
  "Note<sup>a footnote</sup> and x~i\\~j~ with **bold^2^**",
  "**Bold with a [link](https://cilica.app) inside** and plain",
  "##### Fifth level\n\n###### Sixth level",
  "![Whiteboard](https://example.com/board.png)",
  "```\nplain block\n```",
  "```python\ndef f(x):\n    return x + 1\n```",
  // Obsidian links and tags (the serializer must not escape the brackets).
  "See [[Q3 planning]] and [[Note#Head|the alias]].",
  "![[Pricing interview, Marco]]",
  "- [[Item]] in a list\n- [[Second]]",
  "- [ ] task with [[Link]]",
  "[[A]][[B]] and [[folder/Deep note.md]]",
  "#tag at the start and #nested/tag in the middle",
  "Code stays code: `[[not a link]]`",
  "```\n[[not a link]]\n```",
];

describe("editor Markdown round-trip", () => {
  it.each(EXACT)("keeps %j exactly", (markdown) => {
    expect(roundTrip(markdown)).toBe(markdown);
  });

  it("keeps tables (normalised once, then stable)", () => {
    const table = "| Name | Role |\n| --- | --- |\n| Priya | PM |\n| Marco | Eng |";
    const once = roundTrip(table);
    expect(once).toContain("| Priya | PM   |");
    expect(roundTrip(once)).toBe(once);
  });

  it("is stable for every seed note in the mock", async () => {
    const mock = createMockBackend({ latencyMs: 0 });
    const notes = await mock.commands.listNotes();
    expect(notes.length).toBeGreaterThan(5);
    for (const summary of notes) {
      const note = await mock.commands.getNote(summary.id);
      const once = roundTrip(note.body_md);
      expect(roundTrip(once)).toBe(once);
      // Nothing is lost: every word of the source survives.
      const words = (s: string) =>
        s
          .replace(/[^\p{L}\p{N}]+/gu, " ")
          .trim()
          .split(/\s+/)
          .filter(Boolean);
      expect(words(once)).toEqual(expect.arrayContaining(words(note.body_md)));
    }
    mock.dispose?.();
  });

  it("normalises trailing whitespace and blank lines", () => {
    expect(normalizeMarkdown("\n\nA  \n\n\n\nB\n\n")).toBe("A\n\nB");
  });
});

describe("superscript and subscript", () => {
  function marksOf(markdown: string): Array<[string, string[]]> {
    const editor = new Editor({
      extensions: noteExtensions(),
      content: markdown,
      contentType: "markdown",
    });
    const out: Array<[string, string[]]> = [];
    editor.state.doc.descendants((node) => {
      if (node.isText) out.push([node.text ?? "", node.marks.map((m) => m.type.name)]);
    });
    editor.destroy();
    return out;
  }

  it("reads ^sup^ and ~sub~ as marks, and ~~ as strikethrough", () => {
    expect(marksOf("x^2^ H~2~O ~~gone~~")).toEqual([
      ["x", []],
      ["2", ["superscript"]],
      [" H", []],
      ["2", ["subscript"]],
      ["O ", []],
      ["gone", ["strike"]],
    ]);
  });

  it("leaves markers with spaces inside as plain text", () => {
    expect(marksOf("about ~5 minutes, maybe ~10 ^ ok ^")).toEqual([
      ["about ~5 minutes, maybe ~10 ^ ok ^", []],
    ]);
  });

  it("applies ^…^ and ~…~ as you type, and leaves ~~ to strikethrough", () => {
    const element = document.createElement("div");
    const editor = new Editor({ element, extensions: noteExtensions() });
    const type = (text: string) => {
      for (const ch of text) {
        const { from, to } = editor.state.selection;
        const handled = editor.view.someProp("handleTextInput", (f) =>
          f(editor.view, from, to, ch, () => editor.state.tr.insertText(ch, from, to)),
        );
        if (!handled) editor.view.dispatch(editor.state.tr.insertText(ch, from, to));
      }
    };
    type("mc^2^ and H~2~O and ~~gone~~");
    expect(normalizeMarkdown(editor.getMarkdown())).toBe("mc^2^ and H~2~O and ~~gone~~");
    editor.destroy();
  });

  it("ends at its closing marker: typing after a run, or after deleting it, is plain", () => {
    const editor = new Editor({
      element: document.createElement("div"),
      extensions: noteExtensions(),
      content: "x^ab^ and H~2~",
      contentType: "markdown",
    });
    const typeAt = (pos: number, text: string) =>
      editor.chain().setTextSelection(pos).insertContent(text).run();
    typeAt(4, "Q");
    editor.commands.deleteRange({ from: 2, to: 4 });
    typeAt(2, "zz");
    expect(normalizeMarkdown(editor.getMarkdown())).toBe("xzzQ and H~2~");
    // The shortcut still turns it on for what you type next.
    editor.chain().setTextSelection(3).run();
    editor.view.dom.dispatchEvent(
      new KeyboardEvent("keydown", { key: ".", ctrlKey: true, bubbles: true }),
    );
    editor.view.dispatch(editor.state.tr.insertText("3"));
    expect(normalizeMarkdown(editor.getMarkdown())).toBe("xz^3^zQ and H~2~");
    editor.destroy();
  });

  it("reads Pandoc's escaped spaces", () => {
    expect(marksOf("x^a\\ b^")).toEqual([
      ["x", []],
      ["a b", ["superscript"]],
    ]);
  });

  it("writes a run with a space or caret as HTML, which reads back the same", () => {
    const editor = new Editor({ extensions: noteExtensions(), content: "a b^c" });
    editor.chain().selectAll().toggleSuperscript().run();
    expect(normalizeMarkdown(editor.getMarkdown())).toBe("<sup>a b^c</sup>");
    editor.chain().selectAll().toggleSubscript().run();
    expect(normalizeMarkdown(editor.getMarkdown())).toBe("<sub>a b^c</sub>");
    editor.destroy();
    expect(marksOf("<sub>a b^c</sub>")).toEqual([["a b^c", ["subscript"]]]);
  });
});

describe("wiki links in the editor", () => {
  it("keeps a link inside bold or italic, though not the emphasis on the link itself", () => {
    // TipTap's Markdown serializer cannot wrap an inline atom in a mark.
    expect(roundTrip("**Bold [[Link]]** and *[[Other|shown]]*")).toBe(
      "**Bold** [[Link]] and [[Other|shown]]",
    );
  });

  it("parses [[links]] into link nodes with their parts", () => {
    const editor = new Editor({
      extensions: noteExtensions(),
      content: "See [[Plan#Goals|the goals]] and ![[Embed]]",
      contentType: "markdown",
    });
    const links: Record<string, unknown>[] = [];
    editor.state.doc.descendants((n) => {
      if (n.type.name === "wikiLink") links.push(n.attrs);
    });
    expect(links).toEqual([
      { target: "Plan#Goals", alias: "the goals", embed: false },
      { target: "Embed", alias: null, embed: true },
    ]);
    editor.destroy();
  });

  it("turns typed [[Title]] into a link, and Backspace opens it back up", () => {
    const editor = new Editor({
      extensions: noteExtensions(),
      content: "",
      contentType: "markdown",
    });
    editor.commands.setTextSelection(1);
    for (const ch of "Hi [[Plan]]") {
      const { from } = editor.state.selection;
      const handled = editor.view.someProp("handleTextInput", (f) =>
        f(editor.view, from, from, ch, () => editor.state.tr.insertText(ch, from, from)),
      );
      if (!handled) editor.view.dispatch(editor.state.tr.insertText(ch, from, from));
    }
    expect(normalizeMarkdown(editor.getMarkdown())).toBe("Hi [[Plan]]");
    let link = 0;
    editor.state.doc.descendants((n) => {
      if (n.type.name === "wikiLink") link++;
    });
    expect(link).toBe(1);
    editor.commands.keyboardShortcut("Backspace");
    expect(editor.state.doc.textContent).toBe("Hi [[Plan]");
    editor.destroy();
  });

  it("suggests while typing [[ and inserts the picked note", () => {
    const changes: (string | null)[] = [];
    // Set after the editor exists, as LinkSuggest does once it mounts.
    let handlers: SuggestHandlers | null = null;
    const editor = new Editor({
      extensions: noteExtensions({ suggest: () => handlers }),
      content: "",
      contentType: "markdown",
    });
    handlers = {
      onChange: (state) => {
        changes.push(state ? state.query : null);
      },
      onKey: () => false,
    };
    editor.commands.setTextSelection(1);
    editor.commands.insertContent("Talk to [[Pri");
    expect(changes.at(-1)).toBe("Pri");
    const { from } = editor.state.selection;
    editor.commands.insertWikiLink(
      { from: from - 5, to: from },
      { target: "Priya", alias: null, embed: false },
    );
    expect(changes.at(-1)).toBe(null);
    expect(normalizeMarkdown(editor.getMarkdown())).toBe("Talk to [[Priya]]");
    editor.destroy();
  });

  it("keeps suggesting after | and matches on the part before it", () => {
    const changes: ({ query: string; alias: string | null } | null)[] = [];
    let handlers: SuggestHandlers | null = null;
    const editor = new Editor({
      extensions: noteExtensions({ suggest: () => handlers }),
      content: "",
      contentType: "markdown",
    });
    handlers = {
      onChange: (state) => {
        changes.push(state ? { query: state.query, alias: state.alias } : null);
      },
      onKey: () => false,
    };
    editor.commands.setTextSelection(1);
    editor.commands.insertContent("See [[Pri|");
    expect(changes.at(-1)).toEqual({ query: "Pri", alias: "" });
    editor.commands.insertContent("her");
    expect(changes.at(-1)).toEqual({ query: "Pri", alias: "her" });
    editor.commands.insertContent("|");
    expect(changes.at(-1)).toBe(null);
    editor.destroy();
  });
});

describe("find in note", () => {
  it("finds case-insensitive matches across marks", () => {
    const editor = new Editor({
      extensions: noteExtensions(),
      content: "Hello **wor**ld, hello again",
      contentType: "markdown",
    });
    const matches = findMatches(editor.state.doc, "HELLO");
    expect(matches).toHaveLength(2);
    expect(findMatches(editor.state.doc, "world")).toHaveLength(1);
    expect(findMatches(editor.state.doc, "  ")).toHaveLength(0);
    editor.commands.setFindQuery("hello");
    editor.commands.findNext();
    const { from, to } = editor.state.selection;
    expect(editor.state.doc.textBetween(from, to)).toBe("hello");
    editor.destroy();
  });

  it("tells Markdown source from ordinary prose", () => {
    expect(looksLikeMarkdown("# Plan\n\n- one\n- two")).toBe(true);
    expect(looksLikeMarkdown("```js\nlet a = 1;\n```")).toBe(true);
    expect(looksLikeMarkdown("| a | b |\n| --- | --- |")).toBe(true);
    expect(looksLikeMarkdown("See **this** and [that](https://example.com).")).toBe(true);
    expect(looksLikeMarkdown("Just a sentence, with commas - and a dash.")).toBe(false);
    expect(looksLikeMarkdown("3 * 4 * 5 is 60")).toBe(false);
    expect(looksLikeMarkdown("")).toBe(false);
  });

  it("turns [ ] at the start of a bullet into a task", () => {
    const editor = new Editor({
      extensions: noteExtensions(),
      content: "- item",
      contentType: "markdown",
    });
    editor.commands.setTextSelection(3);
    for (const ch of "[ ] ") {
      const { from } = editor.state.selection;
      const handled = editor.view.someProp("handleTextInput", (f) =>
        f(editor.view, from, from, ch, () => editor.state.tr.insertText(ch, from, from)),
      );
      if (!handled) editor.view.dispatch(editor.state.tr.insertText(ch, from, from));
    }
    expect(normalizeMarkdown(editor.getMarkdown())).toBe("- [ ] item");
    editor.destroy();
  });
});
