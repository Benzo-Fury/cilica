/**
 * The rules for open notes as tabs, kept pure so they can be tested without a window.
 * The router still holds the one view on screen; these functions say how the tabs follow it.
 *
 * What becomes a tab follows what you are doing with the note:
 * - Looking: a note you open to read takes the preview tab. There is at most one, shown in
 *   italics, and the next note you look at takes its place, so clicking through the sidebar or
 *   following links doesn't pile up tabs.
 * - Working: a note you create, write in, rename or record into keeps its tab. Opening another
 *   note then adds a tab next to it instead of replacing it.
 * - Asking: "Open in new tab" (middle-click, or the note's menu) and double-clicking a tab keep it.
 * A note that already has a tab just selects it, so a note is never open twice.
 */

/** The most tabs kept open. Opening another drops the leftmost one that is not on screen. */
export const MAX_TABS = 12;

export interface Bar {
  /** Note ids in bar order. */
  tabs: string[];
  /** The tab a note you only looked at sits in, or null. */
  preview: string | null;
}

/** How a note is being opened: to look at it, or to work on it (new, recording). */
export type Opening = "look" | "work";

/** Keep the tabs in step with the router as it moves from `from` to `to`. */
export function follow(
  bar: Bar,
  from: string | null,
  to: string | null,
  opening: Opening = "look",
): Bar {
  if (!to) return copy(bar);
  if (bar.tabs.includes(to)) return opening === "work" ? keep(bar, to) : copy(bar);
  const current = from && bar.tabs.includes(from) ? from : null;

  if (opening === "work") {
    return { tabs: cap(insertAfter(bar.tabs, current, to), [current, to]), preview: bar.preview };
  }
  // Looking from the preview tab: the next note takes its place.
  if (current && current === bar.preview) {
    return { tabs: bar.tabs.map((id) => (id === current ? to : id)), preview: to };
  }
  // Otherwise the old preview goes, and the new one opens beside where you are.
  const rest = bar.tabs.filter((id) => id !== bar.preview);
  return { tabs: cap(insertAfter(rest, current, to), [current, to]), preview: to };
}

/** Open `id` in a tab of its own right after `current` (or at the end), without switching to it. */
export function addTab(bar: Bar, current: string | null, id: string): Bar {
  if (bar.tabs.includes(id)) return keep(bar, id);
  const at = current && bar.tabs.includes(current) ? current : null;
  return { tabs: cap(insertAfter(bar.tabs, at, id), [current, id]), preview: bar.preview };
}

/** The note in `id` is being worked on: its tab stays when you open another note. */
export function keep(bar: Bar, id: string): Bar {
  return { tabs: [...bar.tabs], preview: bar.preview === id ? null : bar.preview };
}

/**
 * Close `id`. When it is the note on screen, `next` is the note to show instead: the tab to its
 * right, else the one to its left, else null (go Home).
 */
export function closeTab(
  bar: Bar,
  id: string,
  current: string | null,
): { bar: Bar; next: string | null } {
  const at = bar.tabs.indexOf(id);
  if (at < 0) return { bar: copy(bar), next: current };
  const tabs = bar.tabs.filter((t) => t !== id);
  const closed = { tabs, preview: bar.preview === id ? null : bar.preview };
  if (id !== current) return { bar: closed, next: current };
  return { bar: closed, next: tabs[at] ?? tabs[at - 1] ?? null };
}

/** Which way the selection moved along the bar: 1 right, -1 left, 0 when it did not move between tabs. */
export function direction(
  tabs: readonly string[],
  from: string | null,
  to: string | null,
): -1 | 0 | 1 {
  if (!from || !to || from === to) return 0;
  const a = tabs.indexOf(from);
  const b = tabs.indexOf(to);
  if (a < 0 || b < 0) return 0;
  return b > a ? 1 : -1;
}

/** Drop tabs whose notes no longer exist (deleted or moved to Trash). */
export function prune(bar: Bar, exists: (id: string) => boolean): Bar {
  const tabs = bar.tabs.filter(exists);
  return { tabs, preview: bar.preview && tabs.includes(bar.preview) ? bar.preview : null };
}

const copy = (bar: Bar): Bar => ({ tabs: [...bar.tabs], preview: bar.preview });

function insertAfter(tabs: readonly string[], after: string | null, id: string): string[] {
  const at = after ? tabs.indexOf(after) : -1;
  return at >= 0 ? [...tabs.slice(0, at + 1), id, ...tabs.slice(at + 1)] : [...tabs, id];
}

/** Trim to MAX_TABS from the left, never dropping the ids in `keep`. */
function cap(tabs: string[], keep: readonly (string | null)[]): string[] {
  while (tabs.length > MAX_TABS) {
    const drop = tabs.findIndex((t) => !keep.includes(t));
    tabs.splice(drop, 1);
  }
  return tabs;
}
