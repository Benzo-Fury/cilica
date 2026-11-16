/**
 * Superscript and subscript, stored the way Pandoc writes them: `x^2^` and `H~2~O`.
 * A run has no spaces, so `~5 minutes ~10` stays prose; Pandoc's `\ ` escape is read, and a run
 * with a space is written as `<sup>`/`<sub>` instead. `~~strike~~` keeps its meaning: a single
 * `~` is subscript here, where GFM would have read it as strikethrough.
 * The two exclude each other (applying one swaps out the other), and typing `^…^` or `~…~` applies them as you go.
 */
import {
  type MarkdownParseHelpers,
  type MarkdownRendererHelpers,
  type MarkdownToken,
  markInputRule,
  type RenderContext,
} from "@tiptap/core";
import BaseSubscript from "@tiptap/extension-subscript";
import BaseSuperscript from "@tiptap/extension-superscript";

/** `^body^` / `~body~` at the start of `src`: no unescaped spaces, not empty, not `~~`. */
function scriptPattern(marker: "^" | "~"): RegExp {
  const m = `\\${marker}`;
  return new RegExp(`^${m}(?!${m})((?:\\\\[\\s\\S]|[^\\s\\\\${m}])+)${m}(?!${m})`);
}

/** Same run, typed at the end of the text before the caret. */
function scriptInputPattern(marker: "^" | "~"): RegExp {
  const m = `\\${marker}`;
  // A lookbehind, not a group: the rule removes everything it matched besides the body.
  return new RegExp(`(?<![${m}\\\\])${m}((?:\\\\[\\s\\S]|[^\\s\\\\${m}])+)${m}$`);
}

/** Drops the backslash from escapes inside a run (`\ `, `\^`, `\~`). */
export function unescapeScript(text: string): string {
  return text.replace(/\\([\s\S])/g, "$1");
}

function scriptMarkdown(name: "superscript" | "subscript", marker: "^" | "~") {
  const pattern = scriptPattern(marker);
  return {
    markdownTokenName: name,
    markdownTokenizer: {
      name,
      level: "inline" as const,
      start: (src: string) => {
        let at = src.indexOf(marker);
        // Skip `~~` so strikethrough is still found by GFM.
        while (at >= 0 && (src[at + 1] === marker || (at > 0 && src[at - 1] === marker))) {
          at = src.indexOf(marker, at + 2);
        }
        return at;
      },
      tokenize: (src: string) => {
        const match = pattern.exec(src);
        if (!match) return undefined;
        return { type: name, raw: match[0], text: unescapeScript(match[1] ?? "") };
      },
    },
    parseMarkdown: (token: MarkdownToken, helpers: MarkdownParseHelpers) =>
      helpers.applyMark(name, [helpers.createTextNode(String(token.text ?? ""))]),
    // The serializer asks for the delimiters around a placeholder and passes the run's text in
    // `meta.markText`. A run the short form cannot hold (a space, or a `^` the serializer
    // leaves unescaped) is written as HTML, which reads back into the same mark.
    renderMarkdown: (
      node: Parameters<MarkdownRendererHelpers["renderChildren"]>[0],
      h: MarkdownRendererHelpers,
      ctx: RenderContext,
    ) => {
      const inner = h.renderChildren(node);
      const markText: unknown = ctx.meta?.markText;
      const text = typeof markText === "string" ? markText : inner;
      const tag = name === "superscript" ? "sup" : "sub";
      return /[\s^]/.test(text) ? `<${tag}>${inner}</${tag}>` : `${marker}${inner}${marker}`;
    },
  };
}

export const Superscript = BaseSuperscript.extend({
  excludes: "subscript",
  // The markers are hidden, so the caret at a run's end is past its closing `^`: typing there,
  // or after deleting the whole run, is plain text. ⌘. still turns it on for what comes next.
  inclusive: false,
  ...scriptMarkdown("superscript", "^"),
  addKeyboardShortcuts() {
    return { "Mod-.": () => this.editor.chain().unsetSubscript().toggleSuperscript().run() };
  },
  addInputRules() {
    return [markInputRule({ find: scriptInputPattern("^"), type: this.type })];
  },
});

export const Subscript = BaseSubscript.extend({
  excludes: "superscript",
  inclusive: false,
  ...scriptMarkdown("subscript", "~"),
  // ⌘, opens Settings, so subscript sits on ⌘⇧, beside superscript's ⌘.
  addKeyboardShortcuts() {
    return { "Mod-Shift-,": () => this.editor.chain().unsetSuperscript().toggleSubscript().run() };
  },
  addInputRules() {
    return [markInputRule({ find: scriptInputPattern("~"), type: this.type })];
  },
});
