/**
 * The note image node. Images live as files (`attachments/<hash>.png`) and the Markdown keeps
 * Obsidian's syntax, so a note opens the same there:
 *
 *   ![Whiteboard|center|400](attachments/3f2a….png "Caption")
 *
 * The alt text carries the placement (`left` is the default and never written) and the width
 * in px, which Obsidian reads too; the Markdown title is the caption shown under the image.
 * An image shown from memory while it is saved (a `blob:` source) is never written out; one
 * being downloaded keeps its web address until the copy is saved.
 */
import type { NodeViewRenderer } from "@tiptap/core";
import Image, { type ImageOptions } from "@tiptap/extension-image";
import { type EditorState, NodeSelection, type Transaction } from "@tiptap/pm/state";

export type ImageAlign = "left" | "center" | "right";

export interface ImageAttrs {
  src: string | null;
  alt: string | null;
  /** The caption, stored as the Markdown title. */
  title: string | null;
  /** Width in CSS px; null shows the image at its own size, never wider than the column. */
  width: number | null;
  align: ImageAlign | null;
  /** Set while the image is being saved or downloaded. */
  pending: string | null;
}

const ALIGNS: readonly ImageAlign[] = ["left", "center", "right"];
/** Smallest width a person can drag an image to. */
export const MIN_IMAGE_WIDTH = 48;

/** `Whiteboard|center|400` → its parts. Obsidian's `400x300` keeps the width. */
export function parseImageAlt(raw: string | null | undefined): {
  alt: string;
  align: ImageAlign | null;
  width: number | null;
} {
  const parts = (raw ?? "").split("|");
  let width: number | null = null;
  let align: ImageAlign | null = null;
  const size = parts.length > 1 ? /^\s*(\d{1,5})(?:x\d{1,5})?\s*$/.exec(parts.at(-1) ?? "") : null;
  if (size) {
    width = Number(size[1]) || null;
    parts.pop();
  }
  const last = parts.length > 1 ? (parts.at(-1) ?? "").trim().toLowerCase() : "";
  if ((ALIGNS as readonly string[]).includes(last)) {
    align = last as ImageAlign;
    parts.pop();
  }
  return { alt: parts.join("|"), align: align === "left" ? null : align, width };
}

/** The alt text as written: `alt|center|400`. */
export function formatImageAlt(alt: string | null, align: ImageAlign | null, width: number | null) {
  const parts = [(alt ?? "").replace(/[[\]]/g, "")];
  if (align && align !== "left") parts.push(align);
  if (width && width > 0) parts.push(String(Math.round(width)));
  return parts.join("|");
}

/** True for a source that has to be saved as a file before it can stay in a note. */
export function needsSaving(src: string | null | undefined): boolean {
  return !!src && /^(https?:|data:image\/)/i.test(src);
}

/** Never written out: shown from memory while the real file is saved. */
const isTransient = (src: string) => src.startsWith("blob:");

function renderImage(attrs: ImageAttrs): string {
  const src = attrs.src ?? "";
  if (!src || isTransient(src)) return "";
  const alt = formatImageAlt(attrs.alt, attrs.align, attrs.width);
  // Spaces would end the destination early; Obsidian and CommonMark both read <…> paths.
  const dest = /[\s()]/.test(src) ? `<${src}>` : src;
  const title = attrs.title?.trim();
  return title ? `![${alt}](${dest} "${title.replace(/"/g, '\\"')}")` : `![${alt}](${dest})`;
}

/**
 * Move the selected image one block up or down (⌥↑ / ⌥↓). Returns false when the selection is
 * not an image or it is already at that end.
 */
export function moveSelectedImage(
  state: EditorState,
  direction: -1 | 1,
  dispatch?: (tr: Transaction) => void,
): boolean {
  const { selection } = state;
  if (!(selection instanceof NodeSelection) || selection.node.type.name !== "image") return false;
  const $pos = state.doc.resolve(selection.from);
  const parent = $pos.parent;
  const index = $pos.index();
  const swapWith = index + direction;
  if (swapWith < 0 || swapWith >= parent.childCount) return false;
  if (!dispatch) return true;
  const node = selection.node;
  const sibling = parent.child(swapWith);
  const tr = state.tr.delete(selection.from, selection.to);
  const target =
    direction === -1 ? selection.from - sibling.nodeSize : selection.from + sibling.nodeSize;
  tr.insert(target, node);
  tr.setSelection(NodeSelection.create(tr.doc, target));
  dispatch(tr.scrollIntoView());
  return true;
}

/** Move the image at `from` so it sits before the block that starts at `to` (drag to move). */
export function moveImageTo(state: EditorState, from: number, to: number): Transaction | null {
  const node = state.doc.nodeAt(from);
  if (node?.type.name !== "image") return null;
  if (to === from || to === from + node.nodeSize) return null;
  const tr = state.tr.delete(from, from + node.nodeSize);
  const target = tr.mapping.map(to);
  tr.insert(target, node);
  tr.setSelection(NodeSelection.create(tr.doc, target));
  return tr;
}

export interface NoteImageOptions {
  /** The React view used in the app; without it images render as a plain `<img>` (tests). */
  view: NodeViewRenderer | null;
}

export const NoteImage = Image.extend<ImageOptions & NoteImageOptions>({
  // Moved with the handle (pointer drag) or ⌥↑/⌥↓, not the browser's own drag and drop.
  draggable: false,

  addOptions() {
    return {
      ...(this.parent?.() as ImageOptions),
      inline: false,
      allowBase64: true,
      resize: false,
      view: null,
    };
  },

  addAttributes() {
    return {
      src: { default: null },
      alt: { default: null },
      title: { default: null },
      width: {
        default: null,
        parseHTML: (el) => {
          const value = Number.parseInt(el.getAttribute("width") ?? "", 10);
          return Number.isFinite(value) && value > 0 ? value : null;
        },
        renderHTML: (attrs) => (attrs.width ? { width: attrs.width } : {}),
      },
      align: {
        default: null,
        parseHTML: (el) => {
          const value = el.getAttribute("data-align");
          return value === "center" || value === "right" ? value : null;
        },
        renderHTML: (attrs) => (attrs.align ? { "data-align": attrs.align } : {}),
      },
      pending: { default: null, rendered: false },
      // Images pasted from the web come with sizes in their HTML; ours lives in `width`.
      height: { default: null, rendered: false, parseHTML: () => null },
    };
  },

  parseMarkdown: (token, helpers) => {
    const { alt, align, width } = parseImageAlt(token.text);
    return helpers.createNode("image", {
      src: token.href,
      title: token.title || null,
      alt: alt || null,
      align,
      width,
    });
  },

  renderMarkdown: (node) => renderImage(node.attrs as ImageAttrs),

  addNodeView() {
    return this.options.view;
  },

  addKeyboardShortcuts() {
    return {
      "Alt-ArrowUp": () => moveSelectedImage(this.editor.state, -1, this.editor.view.dispatch),
      "Alt-ArrowDown": () => moveSelectedImage(this.editor.state, 1, this.editor.view.dispatch),
    };
  },
});
