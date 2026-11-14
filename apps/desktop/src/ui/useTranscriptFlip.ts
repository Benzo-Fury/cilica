/**
 * Smooth moves in a live transcript, so lines glide instead of jumping (FLIP: note where things
 * were, let them land, then animate from the old place to the new one on the `smooth` spring).
 *
 * - Rows (`li[data-key][data-start]`) that shift because a row above grew or shrank slide there.
 * - Words of a "Pending" line glide to wherever they land: into the speaker's turn above, or a
 *   row of their own. They move as themselves; nothing is copied.
 * - "Pending"'s avatar and name fade out on top of the transcript, taking no space, when its last
 *   line leaves.
 *
 * Runs after every commit, so a row the list re-measures in the same frame is corrected before
 * anything is painted. Rows first drawn this frame (scrolled into view) never animate. Reduce
 * motion turns it off.
 */
import { type RefObject, useLayoutEffect, useRef } from "react";

type Point = { x: number; y: number };

interface Snapshot {
  /** Row key → its top in the list, and the frame it first appeared in. */
  rows: Map<string, { y: number; since: number }>;
  /** `segmentId:wordIndex` → the word's row, where it is laid out in that row, where that put it
   * on screen (list coordinates, its own glide left out), and the frame it first appeared in. */
  words: Map<string, { row: string; inRow: Point; drawn: Point; since: number }>;
  /** Pending rows' avatar and name, to fade out when the row goes. */
  heads: Map<string, { y: number; ghost: HTMLElement }>;
}

const PENDING_ROW = ".nt-utt-pending";

/** The translation an element is drawn at right now (its running animations included). */
function shift(el: Element): Point {
  const t = getComputedStyle(el).transform;
  if (!t || t === "none") return { x: 0, y: 0 };
  const m = new DOMMatrixReadOnly(t);
  return { x: m.m41, y: m.m42 };
}

/** Where `el` is laid out inside its row, ignoring every transform. */
function layoutIn(el: HTMLElement, li: HTMLElement): Point {
  let x = 0;
  let y = 0;
  let node: HTMLElement | null = el;
  while (node && node !== li) {
    x += node.offsetLeft;
    y += node.offsetTop;
    node = node.offsetParent as HTMLElement | null;
  }
  return { x, y };
}

function springOf(el: Element) {
  const style = getComputedStyle(el);
  return {
    easing: style.getPropertyValue("--spring-smooth").trim() || "ease-out",
    duration: Number.parseFloat(style.getPropertyValue("--spring-smooth-dur")) || 380,
  };
}

export function useTranscriptFlip(listRef: RefObject<HTMLElement | null>, enabled: boolean) {
  const last = useRef<Snapshot | null>(null);
  const running = useRef(new WeakMap<Element, Animation>());
  const frame = useRef(0);
  const tick = useRef(0);

  useLayoutEffect(() => {
    const list = listRef.current;
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (!list || !enabled || reduced) {
      last.current = null;
      return;
    }
    // Count painted frames, so rows that only just appeared are left alone.
    if (!tick.current)
      tick.current = requestAnimationFrame(() => {
        tick.current = 0;
        frame.current += 1;
      });

    const before = last.current;
    const now: Snapshot = { rows: new Map(), words: new Map(), heads: new Map() };
    const { easing, duration } = springOf(list);

    /** Glide `el` from `by` away (plus wherever it is mid-glide) back to its place. */
    const glide = (el: Element, by: Point) => {
      // Not moved since last time: a glide under way carries on untouched.
      if (Math.abs(by.x) < 0.5 && Math.abs(by.y) < 0.5) return;
      const from = shift(el);
      const prev = running.current.get(el);
      const x = by.x + (prev ? from.x : 0);
      const y = by.y + (prev ? from.y : 0);
      prev?.cancel();
      if (Math.abs(x) < 0.5 && Math.abs(y) < 0.5) return;
      running.current.set(
        el,
        el.animate([{ transform: `translate(${x}px, ${y}px)` }, { transform: "none" }], {
          duration,
          easing,
        }),
      );
    };

    // ── Rows ──
    const rowEls = list.querySelectorAll<HTMLElement>(":scope > li[data-key]");
    for (const li of rowEls) {
      const key = li.dataset.key as string;
      const y = Number(li.dataset.start);
      const prev = before?.rows.get(key);
      const since = prev?.since ?? frame.current;
      now.rows.set(key, { y, since });
      const row = li.firstElementChild;
      if (row && prev && since < frame.current && prev.y !== y) glide(row, { x: 0, y: prev.y - y });
      else if (row && prev && prev.y !== y) {
        // Re-measured before it was ever painted: put it straight where it belongs.
        running.current.get(row)?.cancel();
      }
    }

    // ── Words of pending lines (and of lines that were pending a moment ago) ──
    const tracked = new Set<string>();
    for (const el of list.querySelectorAll<HTMLElement>(`${PENDING_ROW} [data-segment-id]`))
      tracked.add(el.dataset.segmentId as string);
    for (const key of before?.words.keys() ?? []) tracked.add(key.slice(0, key.lastIndexOf(":")));
    for (const id of tracked) {
      const seg = list.querySelector<HTMLElement>(`[data-segment-id="${CSS.escape(id)}"]`);
      const li = seg?.closest<HTMLElement>("li[data-key]");
      if (!seg || !li) continue;
      const row = li.dataset.key as string;
      // The row's own glide (just started above if it moved) carries the words with it.
      const rowShift = li.firstElementChild ? shift(li.firstElementChild) : { x: 0, y: 0 };
      const top = Number(li.dataset.start);
      const words = seg.querySelectorAll<HTMLElement>(".nt-tword");
      words.forEach((word, i) => {
        const key = `${id}:${i}`;
        const inRow = layoutIn(word, li);
        const drawn = { x: inRow.x + rowShift.x, y: top + inRow.y + rowShift.y };
        const was = before?.words.get(key);
        // A word first laid out this frame was never drawn there: nothing to glide from. In the
        // same row it only follows changes inside the row; moved to another row, it glides from
        // where it was drawn.
        if (was && was.since < frame.current)
          glide(
            word,
            was.row === row
              ? { x: was.inRow.x - inRow.x, y: was.inRow.y - inRow.y }
              : { x: was.drawn.x - drawn.x, y: was.drawn.y - drawn.y },
          );
        // Only lines still pending are remembered for the next move.
        if (seg.closest(PENDING_ROW))
          now.words.set(key, { row, inRow, drawn, since: was?.since ?? frame.current });
      });
    }

    // ── Pending's avatar and name, faded out on top when its row goes ──
    for (const li of rowEls) {
      const row = li.firstElementChild as HTMLElement | null;
      if (!row?.matches(PENDING_ROW)) continue;
      const ghost = row.cloneNode(true) as HTMLElement;
      ghost.querySelector(".nt-utt-text")?.remove();
      now.heads.set(li.dataset.key as string, { y: Number(li.dataset.start), ghost });
    }
    for (const [key, head] of before?.heads ?? []) {
      if (now.heads.has(key)) continue;
      const { ghost, y } = head;
      ghost.classList.add("nt-utt-ghost");
      ghost.setAttribute("aria-hidden", "true");
      list.append(ghost);
      ghost
        .animate(
          [
            { opacity: 1, transform: `translateY(${y}px)` },
            { opacity: 0, transform: `translateY(${y - 6}px)` },
          ],
          { duration: 120, easing: "cubic-bezier(0.55, 0, 1, 0.45)", fill: "forwards" },
        )
        .finished.finally(() => ghost.remove());
    }

    last.current = now;
  });

  useLayoutEffect(
    () => () => {
      if (tick.current) cancelAnimationFrame(tick.current);
      tick.current = 0;
    },
    [],
  );
}
