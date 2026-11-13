/**
 * Pure helpers for the formatting toolbar that floats over a text selection: where it goes,
 * when it shows, and how a typed link becomes an href. Kept free of React and the DOM so
 * they can be tested on their own.
 */
import type { EditorState } from "@tiptap/pm/state";
import { TextSelection } from "@tiptap/pm/state";

export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface Placement {
  left: number;
  top: number;
  /** Which side of the selection the bar sits on (it grows from that edge). */
  side: "above" | "below";
}

/**
 * Centre the bar over the selection, `gap` px above it; flip below when there is no room above,
 * and keep it `cilica` px inside the viewport.
 */
export function placeToolbar(
  selection: Box,
  bar: { width: number; height: number },
  viewport: { width: number; height: number },
  gap = 8,
  margin = 8,
): Placement {
  const centre = (selection.left + selection.right) / 2;
  const maxLeft = Math.max(margin, viewport.width - bar.width - margin);
  const left = Math.min(Math.max(centre - bar.width / 2, margin), maxLeft);
  const above = selection.top - gap - bar.height;
  if (above >= margin) return { left, top: above, side: "above" };
  const below = selection.bottom + gap;
  // Neither side fits (a selection taller than the window): pin it to the top edge.
  if (below + bar.height > viewport.height - margin) return { left, top: margin, side: "above" };
  return { left, top: below, side: "below" };
}

/** A real run of text is selected, outside code blocks, in an editable editor. */
export function hasFormattableSelection(state: EditorState, editable: boolean): boolean {
  if (!editable) return false;
  const { selection } = state;
  if (selection.empty || !(selection instanceof TextSelection)) return false;
  if (selection.$from.parent.type.spec.code || selection.$to.parent.type.spec.code) return false;
  return state.doc.textBetween(selection.from, selection.to, " ").trim().length > 0;
}

/**
 * What the person typed in the link field, as an href. Empty means "remove the link".
 * Bare domains get https; mail addresses get mailto; anything with a scheme or that starts
 * with `/` or `#` is kept as typed.
 */
export function normalizeHref(input: string): string {
  const value = input.trim();
  if (!value) return "";
  const hasScheme = /^[a-z][a-z0-9+.-]*:\/\//i.test(value) || /^(mailto|tel):/i.test(value);
  if (hasScheme || value.startsWith("/") || value.startsWith("#")) {
    return value;
  }
  if (/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value)) return `mailto:${value}`;
  return `https://${value}`;
}
