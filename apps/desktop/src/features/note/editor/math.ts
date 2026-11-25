/**
 * Equations in a note, stored the way Obsidian, Typora and Bear write them: `$E = mc^2$` inside
 * a sentence and a `$$ … $$` block on its own lines. The LaTeX is drawn with KaTeX, which ships
 * with the app, so equations draw offline and nothing leaves the machine.
 *
 * Dollar signs follow Pandoc's rule, so prices stay prices: an opening `$` must be followed by a
 * non-space, the closing `$` must follow a non-space, and when the body starts with a digit the
 * closing `$` must not come right before one ("It costs $5 and $10" is text, `$x$2nd` is not). `\$` is always a plain dollar sign, and a plain `$` that would
 * read back as an equation is written as `\$`.
 *
 * Both kinds are atoms. Clicking one, or pressing Enter while it is selected, asks the note view
 * to open the equation editor (`MathHandlers.onEdit`). Typing `$x$` makes an inline equation;
 * `$$` and Enter (or a space) on an empty line makes a block.
 */

import type { Editor } from "@tiptap/core";
import { Extension, InputRule, mergeAttributes, Node } from "@tiptap/core";
import type { Node as PMNode } from "@tiptap/pm/model";
import { NodeSelection, TextSelection } from "@tiptap/pm/state";
import { renderMath } from "@/lib/math";

// ───────── Syntax ─────────

/** An inline equation at the start of `src`; the body is group 1. */
const INLINE = /^\$(?![\s$])((?:\\[\s\S]|[^\\$\n])+?)\$/;
/** The same, typed at the end of the text before the caret. */
const INLINE_TYPED = /(?<![\\$])\$(?![\s$])((?:\\[\s\S]|[^\\$\n])+?)\$$/;
/** `$$…$$` on one line, or `$$` lines around the LaTeX. */
const BLOCK_ONE_LINE = /^ {0,3}\$\$(?!\$)([^\n]+?)\$\$[ \t]*(?:\n+|$)/;
const BLOCK_LINES = /^ {0,3}\$\$[ \t]*\n([\s\S]*?)\n? {0,3}\$\$[ \t]*(?:\n+|$)/;

/** True when the `$` at `at` is escaped by an odd run of backslashes. */
function escaped(src: string, at: number): boolean {
  let n = 0;
  for (let i = at - 1; i >= 0 && src[i] === "\\"; i--) n++;
  return n % 2 === 1;
}

/** The inline equation at the start of `src`, if there is one: its Markdown and its LaTeX. */
export function matchInlineMath(src: string): { raw: string; latex: string } | null {
  const m = INLINE.exec(src);
  if (!m) return null;
  const latex = m[1] ?? "";
  // The closing `$` must follow a non-space, and a price ("$5 and $10") isn't an equation.
  if (/\s$/.test(latex)) return null;
  if (/^\d/.test(latex) && /^\d/.test(src.slice(m[0].length))) return null;
  return { raw: m[0], latex };
}

/** Where the next inline equation could start in `src`, or -1. */
export function inlineMathStart(src: string): number {
  for (let at = src.indexOf("$"); at >= 0; at = src.indexOf("$", at + 1)) {
    if (escaped(src, at) || src[at - 1] === "$") continue;
    if (matchInlineMath(src.slice(at))) return at;
  }
  return -1;
}

/** The block equation at the start of `src`, if there is one. */
export function matchBlockMath(src: string): { raw: string; latex: string } | null {
  const one = BLOCK_ONE_LINE.exec(src);
  if (one) return { raw: one[0], latex: (one[1] ?? "").trim() };
  const lines = BLOCK_LINES.exec(src);
  if (lines) return { raw: lines[0], latex: lines[1] ?? "" };
  return null;
}

/**
 * A plain `$` in text that would read back as an equation, written as `\$`: one that opens an
 * inline equation, and every `$` in a `$$` run (which could open or close a block).
 */
export function escapeDollars(text: string): string {
  let out = "";
  let last = 0;
  for (let at = text.indexOf("$"); at >= 0; at = text.indexOf("$", at + 1)) {
    if (escaped(text, at)) continue;
    const doubled = text[at - 1] === "$" || text[at + 1] === "$";
    if (!doubled && !matchInlineMath(text.slice(at))) continue;
    out += `${text.slice(last, at)}\\$`;
    last = at + 1;
  }
  return out + text.slice(last);
}

/** The Markdown for an equation. */
export function mathMarkdown(latex: string, display: boolean): string {
  if (display) return `$$\n${latex.trim()}\n$$`;
  // An inline equation is one line in Markdown.
  const body = latex.trim().replace(/\s*\n\s*/g, " ");
  return body ? `$${body}$` : "";
}

// ───────── Nodes ─────────

/** What the note view needs to open the equation editor over an equation. */
export interface MathEditRequest {
  pos: number;
  latex: string;
  display: boolean;
  /** The equation's element, to place the editor under. */
  dom: HTMLElement;
  /** Just inserted: cancelling the editor removes it again. */
  isNew: boolean;
}

export interface MathHandlers {
  onEdit: (request: MathEditRequest) => void;
}

export interface MathOptions {
  /** Who opens the equation editor. Without it equations still draw and round-trip. */
  handlers: () => MathHandlers | null;
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    math: {
      /** Insert an equation at the caret and open the editor on it. */
      insertMath: (options: { display: boolean; latex?: string }) => ReturnType;
      /** Open the editor on the equation at `pos`. */
      editMath: (pos: number, isNew?: boolean) => ReturnType;
      /** Replace the LaTeX of the equation at `pos`. */
      setMathLatex: (pos: number, latex: string) => ReturnType;
      /** Remove the equation at `pos`. */
      removeMath: (pos: number) => ReturnType;
    };
  }
}

const isMath = (node: PMNode | null | undefined): node is PMNode =>
  node?.type.name === "mathInline" || node?.type.name === "mathBlock";

/** Ask the note view to open the editor on the equation at `pos`. */
function openEditor(
  editor: Editor,
  handlers: MathOptions["handlers"],
  pos: number,
  isNew: boolean,
) {
  const node = editor.state.doc.nodeAt(pos);
  const dom = editor.view.nodeDOM(pos);
  const target = handlers();
  if (!isMath(node) || !(dom instanceof HTMLElement) || !target || !editor.isEditable) return false;
  target.onEdit({
    pos,
    latex: String(node.attrs.latex ?? ""),
    display: node.type.name === "mathBlock",
    dom,
    isNew,
  });
  return true;
}

/** Draws one equation into `dom`: KaTeX, an empty placeholder, or the LaTeX in red with the reason. */
function draw(dom: HTMLElement, latex: string, display: boolean) {
  dom.replaceChildren();
  dom.classList.remove("is-error", "is-empty");
  dom.removeAttribute("title");
  if (!latex.trim()) {
    dom.classList.add("is-empty");
    dom.textContent = "Empty equation";
    dom.setAttribute("aria-label", "Empty equation. Press Enter to edit.");
    return;
  }
  const result = renderMath(latex, display);
  if (result.ok) {
    dom.innerHTML = result.html;
    dom.setAttribute("aria-label", `Equation ${latex}. Press Enter to edit.`);
    return;
  }
  dom.classList.add("is-error");
  dom.title = result.error;
  const source = document.createElement("code");
  source.className = "nt-math-source";
  source.textContent = display ? latex : `$${latex}$`;
  dom.append(source);
  if (display) {
    const why = document.createElement("span");
    why.className = "nt-math-reason";
    why.textContent = `Can’t show this equation: ${result.error}. Click it to fix the LaTeX.`;
    dom.append(why);
  }
  dom.setAttribute(
    "aria-label",
    `Equation that can’t be shown: ${result.error}. Press Enter to fix it.`,
  );
}

function mathNodeView(display: boolean, handlers: MathOptions["handlers"]) {
  return ({
    node,
    getPos,
    editor,
  }: {
    node: PMNode;
    getPos: () => number | undefined;
    editor: Editor;
  }) => {
    let current = node;
    const dom = document.createElement(display ? "div" : "span");
    dom.className = display ? "nt-math nt-math-block" : "nt-math nt-math-inline";
    dom.contentEditable = "false";
    dom.setAttribute("role", "button");
    dom.dataset.latex = String(node.attrs.latex ?? "");
    draw(dom, dom.dataset.latex, display);
    dom.addEventListener("click", (event) => {
      const pos = getPos();
      if (typeof pos !== "number") return;
      event.preventDefault();
      openEditor(editor, handlers, pos, false);
    });
    return {
      dom,
      update: (next: PMNode) => {
        if (next.type !== current.type) return false;
        current = next;
        const latex = String(next.attrs.latex ?? "");
        if (latex !== dom.dataset.latex) {
          dom.dataset.latex = latex;
          draw(dom, latex, display);
        }
        return true;
      },
      selectNode: () => dom.classList.add("is-selected"),
      deselectNode: () => dom.classList.remove("is-selected"),
      ignoreMutation: () => true,
    };
  };
}

const latexAttr = {
  latex: {
    default: "",
    parseHTML: (el: HTMLElement) => el.dataset.latex ?? el.textContent ?? "",
    renderHTML: (attrs: Record<string, unknown>) => ({ "data-latex": String(attrs.latex ?? "") }),
  },
};

export const MathInline = Node.create<MathOptions>({
  name: "mathInline",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  draggable: false,

  addOptions() {
    return { handlers: () => null };
  },

  addAttributes() {
    return latexAttr;
  },

  parseHTML() {
    return [{ tag: "span[data-math-inline]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, { "data-math-inline": "" }),
      `$${node.attrs.latex}$`,
    ];
  },

  renderText({ node }) {
    return mathMarkdown(String(node.attrs.latex ?? ""), false);
  },

  markdownTokenName: "mathInline",
  markdownTokenizer: {
    name: "mathInline",
    level: "inline",
    start: (src: string) => inlineMathStart(src),
    tokenize: (src: string) => {
      const m = matchInlineMath(src);
      return m ? { type: "mathInline", raw: m.raw, latex: m.latex } : undefined;
    },
  },
  parseMarkdown: (token, helpers) =>
    helpers.createNode("mathInline", { latex: String(token.latex ?? "") }),
  renderMarkdown: (node) => mathMarkdown(String(node.attrs?.latex ?? ""), false),

  addNodeView() {
    return mathNodeView(false, this.options.handlers);
  },

  addInputRules() {
    return [
      new InputRule({
        find: INLINE_TYPED,
        handler: ({ state, range, match }) => {
          const latex = match[1] ?? "";
          if (/\s$/.test(latex)) return null;
          state.tr.replaceWith(range.from, range.to, this.type.create({ latex }));
        },
      }),
    ];
  },

  addCommands() {
    const handlers = this.options.handlers;
    return {
      insertMath:
        ({ display, latex = "" }) =>
        ({ editor, tr, dispatch }) => {
          const type = editor.schema.nodes[display ? "mathBlock" : "mathInline"];
          if (!type) return false;
          const { $from, from, to } = tr.selection;
          // Code holds only text.
          if ($from.parent.type.spec.code) return false;
          let pos: number;
          if (display) {
            const parent = $from.parent;
            const emptyParagraph = parent.type.name === "paragraph" && parent.content.size === 0;
            // A list item must start with a paragraph, so there the block goes after it.
            const replaceable = $from
              .node(-1)
              .canReplaceWith($from.index(-1), $from.indexAfter(-1), type);
            if (emptyParagraph && !replaceable) {
              pos = $from.after();
              tr.insert(pos, type.create({ latex }));
            } else if (emptyParagraph) {
              pos = $from.before();
              tr.replaceWith(pos, $from.after(), type.create({ latex }));
            } else {
              // After the block the caret is in.
              pos = $from.after(1);
              tr.insert(pos, type.create({ latex }));
            }
          } else {
            pos = from;
            tr.replaceWith(from, to, type.create({ latex }));
          }
          if (!isMath(tr.doc.nodeAt(pos))) return false;
          tr.setSelection(NodeSelection.create(tr.doc, pos));
          if (dispatch) {
            dispatch(tr);
            // Open once the view has drawn the new node.
            requestAnimationFrame(() => openEditor(editor, handlers, pos, true));
          }
          return true;
        },
      editMath:
        (pos, isNew = false) =>
        ({ editor }) =>
          openEditor(editor, handlers, pos, isNew),
      setMathLatex:
        (pos, latex) =>
        ({ tr, dispatch }) => {
          const node = tr.doc.nodeAt(pos);
          if (!isMath(node)) return false;
          if (dispatch) {
            tr.setNodeMarkup(pos, undefined, { ...node.attrs, latex });
            dispatch(tr);
          }
          return true;
        },
      removeMath:
        (pos) =>
        ({ tr, dispatch }) => {
          const node = tr.doc.nodeAt(pos);
          if (!isMath(node)) return false;
          if (dispatch) {
            tr.delete(pos, pos + node.nodeSize);
            const at = Math.min(pos, tr.doc.content.size);
            try {
              tr.setSelection(TextSelection.near(tr.doc.resolve(at)));
            } catch {
              // Keep ProseMirror's mapped selection.
            }
            dispatch(tr);
          }
          return true;
        },
    };
  },

  addKeyboardShortcuts() {
    const handlers = this.options.handlers;
    return {
      // Enter on a selected equation opens it.
      Enter: ({ editor }) => {
        const { selection } = editor.state;
        if (selection instanceof NodeSelection && isMath(selection.node)) {
          return openEditor(editor, handlers, selection.from, false);
        }
        // `$$` alone on a line, then Enter, starts an equation block.
        const { $from, empty } = selection;
        if (empty && $from.parent.type.name === "paragraph" && $from.parent.textContent === "$$") {
          return editor
            .chain()
            .deleteRange({ from: $from.start(), to: $from.end() })
            .insertMath({ display: true })
            .run();
        }
        return false;
      },
      "Mod-Shift-m": ({ editor }) => {
        const { from, to, empty } = editor.state.selection;
        const latex = empty ? "" : editor.state.doc.textBetween(from, to, " ");
        return editor.commands.insertMath({ display: false, latex });
      },
    };
  },
});

/**
 * The serializer escapes Markdown punctuation in text but not `$`, so a plain `$x$` would read
 * back as an equation. This escapes those. It runs last (lowest priority), once the Markdown
 * extension has set up `editor.markdown`.
 */
export const MathEscape = Extension.create({
  name: "mathEscape",
  priority: 1,
  onBeforeCreate() {
    const manager = this.editor.markdown as unknown as
      | { escapeMarkdownSyntax?: (text: string) => string }
      | undefined;
    const base = manager?.escapeMarkdownSyntax?.bind(manager);
    if (manager && base) manager.escapeMarkdownSyntax = (text) => escapeDollars(base(text));
  },
});

export const MathBlock = Node.create<MathOptions>({
  name: "mathBlock",
  group: "block",
  atom: true,
  selectable: true,
  draggable: false,
  defining: true,

  addOptions() {
    return { handlers: () => null };
  },

  addAttributes() {
    return latexAttr;
  },

  parseHTML() {
    return [{ tag: "div[data-math-block]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    return [
      "div",
      mergeAttributes(HTMLAttributes, { "data-math-block": "" }),
      `$$${node.attrs.latex}$$`,
    ];
  },

  renderText({ node }) {
    return mathMarkdown(String(node.attrs.latex ?? ""), true);
  },

  markdownTokenName: "mathBlock",
  markdownTokenizer: {
    name: "mathBlock",
    level: "block",
    // Only where a block really starts, so `$$` inside a paragraph's lines isn't split off.
    start: (src: string) => {
      for (const m of src.matchAll(/^ {0,3}\$\$/gm)) {
        if (matchBlockMath(src.slice(m.index))) return m.index;
      }
      return -1;
    },
    tokenize: (src: string) => {
      const m = matchBlockMath(src);
      return m ? { type: "mathBlock", raw: m.raw, latex: m.latex } : undefined;
    },
  },
  parseMarkdown: (token, helpers) =>
    helpers.createNode("mathBlock", { latex: String(token.latex ?? "") }),
  renderMarkdown: (node) => mathMarkdown(String(node.attrs?.latex ?? ""), true),

  addNodeView() {
    return mathNodeView(true, this.options.handlers);
  },

  addInputRules() {
    return [
      new InputRule({
        // `$$` and a space at the start of an empty line.
        find: /^\$\$\s$/,
        handler: ({ state, range, chain }) => {
          const $from = state.doc.resolve(range.from);
          if ($from.parent.type.name !== "paragraph") return null;
          chain().deleteRange(range).insertMath({ display: true }).run();
        },
      }),
    ];
  },
});
