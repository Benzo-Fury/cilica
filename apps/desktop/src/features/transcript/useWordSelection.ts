/**
 * Selecting words in the transcript: when the mouse is released over a selection of transcript
 * text, the selection snaps to whole words (a drag that starts or ends mid-word takes the whole
 * word) and the words it covers are returned, for the "Who said this?" menu. Off while the
 * meeting is still being recorded or processed.
 */
import { type RefObject, useCallback, useEffect, useState } from "react";
import type { Segment } from "@/ipc/types";
import { highlightWords } from "@/ui";
import { type SelectedEnd, selectedText, type WordSpan, wordAtEdge, wordSpans } from "./model";

export interface WordSelection {
  from: SelectedEnd;
  to: SelectedEnd;
  /** The selected words, joined by spaces. */
  text: string;
  /** Where they are on the page, for anchoring the menu. */
  range: Range;
}

/** Characters of `el`'s text before the point (`node`, `offset`). */
function charsBefore(el: Element, node: Node, offset: number): number {
  const r = document.createRange();
  r.setStart(el, 0);
  r.setEnd(node, offset);
  return r.toString().length;
}

/** The text node and offset at character `chars` of `el`'s text. */
function pointAt(el: Element, chars: number): [Node, number] {
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  let left = chars;
  let last: Node | null = null;
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const len = n.textContent?.length ?? 0;
    if (left <= len) return [n, left];
    left -= len;
    last = n;
  }
  return last ? [last, last.textContent?.length ?? 0] : [el, 0];
}

export function useWordSelection(
  listRef: RefObject<HTMLElement | null>,
  segments: readonly Segment[],
  enabled: boolean,
) {
  const [selection, setSelection] = useState<WordSelection | null>(null);

  const clear = useCallback((dropPageSelection: boolean) => {
    highlightWords(null);
    if (dropPageSelection) window.getSelection()?.removeAllRanges();
    setSelection(null);
  }, []);

  useEffect(() => {
    const list = listRef.current;
    if (!enabled || !list) return;
    const byId = new Map(segments.map((s) => [s.id, s]));

    const read = (): WordSelection | null => {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed || sel.rangeCount === 0) return null;
      const range = sel.getRangeAt(0);
      if (!list.contains(range.commonAncestorContainer)) return null;
      const lines = [...list.querySelectorAll<HTMLElement>("[data-segment-id]")].filter((el) =>
        range.intersectsNode(el),
      );
      const firstEl = lines[0];
      const lastEl = lines[lines.length - 1];
      if (!firstEl || !lastEl) return null;
      const first = byId.get(firstEl.dataset.segmentId ?? "");
      const last = byId.get(lastEl.dataset.segmentId ?? "");
      if (!first || !last) return null;

      // Where the selection starts and ends in each line's text, then the whole words there.
      const startChars = firstEl.contains(range.startContainer)
        ? charsBefore(firstEl, range.startContainer, range.startOffset)
        : 0;
      const endChars = lastEl.contains(range.endContainer)
        ? charsBefore(lastEl, range.endContainer, range.endOffset)
        : last.text.length;
      let fromWord: WordSpan | null = wordAtEdge(first.text, startChars, "start");
      let fromEl = firstEl;
      let fromLine = first;
      // The selection starts after the last word of its first line: start at the next line.
      if (!fromWord && lines[1]) {
        fromEl = lines[1];
        fromLine = byId.get(fromEl.dataset.segmentId ?? "") ?? first;
        fromWord = wordSpans(fromLine.text)[0] ?? null;
      }
      const toWord = wordAtEdge(last.text, endChars, "end");
      if (!fromWord || !toWord) return null;
      const from = { segmentId: fromLine.id, word: fromWord };
      const to = { segmentId: last.id, word: toWord };
      const text = selectedText(segments, from, to);
      if (!text) return null;

      // Snap the page selection to the whole words.
      const [startNode, startOffset] = pointAt(fromEl, fromWord.start);
      const [endNode, endOffset] = pointAt(lastEl, toWord.end);
      sel.setBaseAndExtent(startNode, startOffset, endNode, endOffset);
      return { from, to, text, range: sel.getRangeAt(0).cloneRange() };
    };

    const onMouseUp = (event: MouseEvent) => {
      const target = event.target as Element | null;
      if (!target || !list.contains(target) || target.closest("button, input, a")) return;
      // Let a double or triple click finish selecting first.
      setTimeout(() => {
        const next = read();
        if (!next) return;
        highlightWords(next.range);
        setSelection(next);
      }, 0);
    };
    document.addEventListener("mouseup", onMouseUp);
    return () => document.removeEventListener("mouseup", onMouseUp);
  }, [listRef, segments, enabled]);

  // Leaving the transcript, or the meeting starting to record again, drops the menu.
  useEffect(() => {
    if (!enabled) clear(false);
  }, [enabled, clear]);
  useEffect(() => () => highlightWords(null), []);

  return { selection, clear };
}
