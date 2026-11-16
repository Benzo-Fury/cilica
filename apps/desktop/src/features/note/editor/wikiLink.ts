/**
 * Obsidian links in the editor: `[[Title]]`, `[[Title#Heading|shown]]` and `![[Title]]` embeds.
 *
 * Each is an inline atom that writes back exactly the Markdown it came from (without this the
 * Markdown serializer escapes the brackets and the link is lost on the first save). It shows the
 * shown text, or the title, styled by what it points at: a note, a saved person, or a note not
 * created yet. Typing `]]` or pasting turns the text into a link; Backspace right after a link
 * turns it back into text to edit. The left arrow into a link from its right (or the right arrow
 * from its left) opens it up as raw `[[…]]` text with the caret inside; moving the caret out
 * of that text turns it back into a link.
 */
import { InputRule, mergeAttributes, Node, PasteRule } from "@tiptap/core";
import type { Node as PMNode } from "@tiptap/pm/model";
import { Plugin, PluginKey, TextSelection } from "@tiptap/pm/state";
import { splitWiki } from "@/lib/links";

export interface WikiLinkAttrs {
  /** The link's target as written: `Title`, `Title#Heading`, `folder/Title.md`. */
  target: string;
  alias: string | null;
  embed: boolean;
}

export type WikiLinkState = "note" | "person" | "missing";

export interface WikiLinkOptions {
  /** What the target is, for styling. */
  resolve: (target: string) => { state: WikiLinkState; preview?: string };
  /** Re-resolve every visible link when this fires (the notes list changed). */
  subscribe: (listener: () => void) => () => void;
  onOpen: (attrs: WikiLinkAttrs) => void;
}

/** The Markdown for a link. */
export function wikiMarkdown({ target, alias, embed }: WikiLinkAttrs): string {
  return `${embed ? "!" : ""}[[${target}${alias !== null ? `|${alias}` : ""}]]`;
}

/** Attributes from a link's inside (`Title#Heading|shown`). */
export function wikiAttrs(inner: string, embed: boolean): WikiLinkAttrs {
  const bar = inner.indexOf("|");
  return {
    target: (bar >= 0 ? inner.slice(0, bar) : inner).replace(/\\$/, "").trim(),
    alias: bar >= 0 ? inner.slice(bar + 1) : null,
    embed,
  };
}

/** What the link shows: its alias, else the title without path or `.md` (and the heading). */
export function wikiLabel({ target, alias }: Pick<WikiLinkAttrs, "target" | "alias">): string {
  if (alias?.trim()) return alias;
  const { page, anchor } = splitWiki(target);
  const name = page.split(/[/\\]/).pop()?.replace(/\.md$/i, "") ?? page;
  const heading = anchor.replace(/^#\^?/, "");
  if (!name) return heading;
  return heading ? `${name} › ${heading}` : name;
}

const LINK = /^(!?)\[\[([^[\]\n]+?)\]\]/;
const LINK_AT_END = /(!?)\[\[([^[\]\n]+?)\]\]$/;
const LINK_GLOBAL = /(!?)\[\[([^[\]\n]+?)\]\]/g;
const LINK_EXACT = /^(!?)\[\[([^[\]\n]+?)\]\]$/;

/** The range of a link opened up by the arrow keys, until the caret leaves it. */
const expandedKey = new PluginKey<{ from: number; to: number } | null>("wikiLinkExpanded");

export const WikiLink = Node.create<WikiLinkOptions>({
  name: "wikiLink",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  draggable: false,

  addOptions() {
    return {
      resolve: () => ({ state: "missing" as const }),
      subscribe: () => () => undefined,
      onOpen: () => undefined,
    };
  },

  addAttributes() {
    return {
      target: { default: "" },
      alias: { default: null },
      embed: { default: false },
    };
  },

  parseHTML() {
    return [
      {
        tag: "span[data-wiki-target]",
        getAttrs: (el) => ({
          target: (el as HTMLElement).dataset.wikiTarget ?? "",
          alias: (el as HTMLElement).dataset.wikiAlias ?? null,
          embed: (el as HTMLElement).dataset.wikiEmbed === "true",
        }),
      },
    ];
  },

  renderHTML({ node, HTMLAttributes }) {
    const attrs = node.attrs as WikiLinkAttrs;
    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        "data-wiki-target": attrs.target,
        "data-wiki-alias": attrs.alias ?? undefined,
        "data-wiki-embed": String(attrs.embed),
      }),
      wikiLabel(attrs),
    ];
  },

  renderText({ node }) {
    return wikiMarkdown(node.attrs as WikiLinkAttrs);
  },

  markdownTokenName: "wikiLink",
  markdownTokenizer: {
    name: "wikiLink",
    level: "inline",
    start: (src: string) => {
      const at = src.indexOf("[[");
      return at > 0 && src[at - 1] === "!" ? at - 1 : at;
    },
    tokenize: (src: string) => {
      const m = LINK.exec(src);
      if (!m) return undefined;
      return { type: "wikiLink", raw: m[0], embed: m[1] === "!", inner: m[2] };
    },
  },
  parseMarkdown: (token, helpers) =>
    helpers.createNode("wikiLink", wikiAttrs(String(token.inner ?? ""), Boolean(token.embed))),
  renderMarkdown: (node) => wikiMarkdown(node.attrs as WikiLinkAttrs),

  addInputRules() {
    return [
      new InputRule({
        find: LINK_AT_END,
        handler: ({ state, range, match }) => {
          const node = this.type.create(wikiAttrs(match[2] ?? "", match[1] === "!"));
          state.tr.replaceWith(range.from, range.to, node);
        },
      }),
    ];
  },

  addPasteRules() {
    return [
      new PasteRule({
        find: LINK_GLOBAL,
        handler: ({ state, range, match }) => {
          const node = this.type.create(wikiAttrs(match[2] ?? "", match[1] === "!"));
          state.tr.replaceWith(range.from, range.to, node);
        },
      }),
    ];
  },

  addProseMirrorPlugins() {
    const type = this.type;
    return [
      new Plugin<{ from: number; to: number } | null>({
        key: expandedKey,
        state: {
          init: () => null,
          apply: (tr, value) => {
            const meta = tr.getMeta(expandedKey) as { from: number; to: number } | null | undefined;
            if (meta !== undefined) return meta;
            if (!value) return null;
            const from = tr.mapping.map(value.from, -1);
            const to = tr.mapping.map(value.to, 1);
            return from < to ? { from, to } : null;
          },
        },
        // Once the caret is outside the opened text, turn it back into a link if it still is one.
        appendTransaction: (_trs, _old, state) => {
          const range = expandedKey.getState(state);
          if (!range) return null;
          const { from, to } = state.selection;
          if (from > range.from && to < range.to) return null;
          const tr = state.tr.setMeta(expandedKey, null);
          const text = state.doc.textBetween(range.from, range.to, "\n");
          const m = LINK_EXACT.exec(text);
          if (m) {
            tr.replaceWith(range.from, range.to, type.create(wikiAttrs(m[2] ?? "", m[1] === "!")));
          }
          return tr;
        },
      }),
    ];
  },

  addKeyboardShortcuts() {
    const expand = (side: "before" | "after") => {
      const { state, view } = this.editor;
      const { selection } = state;
      if (!selection.empty) return false;
      const node = side === "before" ? selection.$from.nodeBefore : selection.$from.nodeAfter;
      if (node?.type !== this.type) return false;
      const text = wikiMarkdown(node.attrs as WikiLinkAttrs);
      const from = side === "before" ? selection.from - node.nodeSize : selection.from;
      const tr = state.tr.replaceWith(from, from + node.nodeSize, state.schema.text(text));
      // Land just inside the brackets: before `]]` from the right, after `[[` from the left.
      const caret = side === "before" ? from + text.length - 2 : from + text.indexOf("[[") + 2;
      tr.setSelection(TextSelection.create(tr.doc, caret));
      tr.setMeta(expandedKey, { from, to: from + text.length });
      view.dispatch(tr);
      return true;
    };
    return {
      ArrowLeft: () => expand("before"),
      ArrowRight: () => expand("after"),
      // Backspace right after a link opens it back up as text, minus its last bracket.
      Backspace: () => {
        const { selection } = this.editor.state;
        if (!selection.empty) return false;
        const before = selection.$from.nodeBefore;
        if (before?.type !== this.type) return false;
        const text = wikiMarkdown(before.attrs as WikiLinkAttrs).slice(0, -1);
        const from = selection.from - before.nodeSize;
        return this.editor.commands.insertContentAt(
          { from, to: selection.from },
          { type: "text", text },
        );
      },
    };
  },

  addNodeView() {
    const options = this.options;
    return ({ node: initial }) => {
      let node: PMNode = initial;
      const dom = document.createElement("span");
      dom.className = "nt-wikilink";
      dom.contentEditable = "false";
      const label = document.createElement("span");
      label.className = "nt-wikilink-label";
      const preview = document.createElement("span");
      preview.className = "nt-wikilink-preview";
      dom.append(label);

      const render = () => {
        const attrs = node.attrs as WikiLinkAttrs;
        const resolved = options.resolve(attrs.target);
        dom.dataset.state = resolved.state;
        dom.dataset.embed = attrs.embed ? "true" : "false";
        label.textContent = wikiLabel(attrs);
        dom.title =
          resolved.state === "missing"
            ? `${wikiLabel({ target: attrs.target, alias: null })} is not created yet. Click to create it.`
            : "";
        dom.setAttribute("role", "link");
        if (attrs.embed && resolved.preview) {
          preview.textContent = resolved.preview;
          if (!preview.isConnected) dom.append(preview);
        } else if (preview.isConnected) {
          preview.remove();
        }
      };
      render();
      const unsubscribe = options.subscribe(render);
      const open = (event: MouseEvent) => {
        if (event.button !== 0) return;
        event.preventDefault();
        event.stopPropagation();
        options.onOpen(node.attrs as WikiLinkAttrs);
      };
      dom.addEventListener("click", open);

      return {
        dom,
        update: (next: PMNode) => {
          if (next.type !== node.type) return false;
          node = next;
          render();
          return true;
        },
        selectNode: () => dom.classList.add("is-selected"),
        deselectNode: () => dom.classList.remove("is-selected"),
        ignoreMutation: () => true,
        destroy: () => {
          unsubscribe();
          dom.removeEventListener("click", open);
        },
      };
    };
  },
});
