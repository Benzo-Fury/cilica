/**
 * Moving an image with its grip. Works with pointer events rather than the browser's drag and
 * drop (which the window's file drop takes over on Windows): past a 4px threshold an accent line
 * marks the gap between blocks it will land in, the page scrolls near its edges, Esc cancels,
 * and on release the image moves there and settles with the bouncy spring. A click without a
 * drag just selects the image.
 */
import type { Editor } from "@tiptap/core";
import { moveImageTo } from "../editor/image";
import { useImageUi } from "./store";

const THRESHOLD = 4;
const EDGE = 48;

interface Gap {
  /** Document position between two top-level blocks. */
  pos: number;
  /** Viewport y of the line. */
  y: number;
}

/** The gaps between top-level blocks, top to bottom, leaving out the image's own two sides. */
function gaps(editor: Editor, from: number): Gap[] {
  const { doc } = editor.state;
  const node = doc.nodeAt(from);
  const to = from + (node?.nodeSize ?? 0);
  const out: Gap[] = [];
  let prevBottom: number | null = null;
  doc.forEach((child, offset) => {
    const dom = editor.view.nodeDOM(offset);
    if (!(dom instanceof HTMLElement)) return;
    const rect = dom.getBoundingClientRect();
    const y = prevBottom === null ? rect.top - 4 : (prevBottom + rect.top) / 2;
    if (offset !== from && offset !== to) out.push({ pos: offset, y });
    prevBottom = rect.bottom;
    // After the last block.
    if (offset + child.nodeSize === doc.content.size && offset !== from) {
      out.push({ pos: doc.content.size, y: rect.bottom + 4 });
    }
  });
  return out;
}

function scroller(el: HTMLElement | null): HTMLElement | null {
  for (let node = el; node; node = node.parentElement) {
    const style = getComputedStyle(node);
    if (/(auto|scroll)/.test(style.overflowY) && node.scrollHeight > node.clientHeight) return node;
  }
  return null;
}

/** Start watching a press on an image's grip. `onDragging` reports when a real drag begins and ends. */
export function startImageDrag(
  editor: Editor,
  pos: number,
  src: string | null,
  event: PointerEvent,
  onDragging: (dragging: boolean) => void,
) {
  if (event.button !== 0 || !editor.isEditable) return;
  const grip = event.target as HTMLElement;
  const startY = event.clientY;
  const startX = event.clientX;
  let dragging = false;
  let target: Gap | null = null;
  let pointerY = startY;
  let frame = 0;
  const line = document.createElement("div");
  line.className = "nt-image-dropline";
  const column = editor.view.dom.getBoundingClientRect();
  const scrollEl = scroller(editor.view.dom);

  const place = () => {
    const all = gaps(editor, pos);
    target = all.reduce<Gap | null>(
      (best, gap) =>
        !best || Math.abs(gap.y - pointerY) < Math.abs(best.y - pointerY) ? gap : best,
      null,
    );
    if (!target) {
      line.remove();
      return;
    }
    Object.assign(line.style, {
      left: `${column.left}px`,
      width: `${column.width}px`,
      top: `${Math.round(target.y - 1)}px`,
    });
    if (!line.isConnected) document.body.append(line);
  };

  // Near the top or bottom of the page, keep scrolling while the pointer stays there.
  const tick = () => {
    frame = 0;
    if (!dragging || !scrollEl) return;
    const box = scrollEl.getBoundingClientRect();
    const speed =
      pointerY < box.top + EDGE
        ? -Math.ceil((box.top + EDGE - pointerY) / 4)
        : pointerY > box.bottom - EDGE
          ? Math.ceil((pointerY - (box.bottom - EDGE)) / 4)
          : 0;
    if (speed !== 0) {
      scrollEl.scrollTop += speed;
      place();
      frame = requestAnimationFrame(tick);
    }
  };

  const move = (e: PointerEvent) => {
    pointerY = e.clientY;
    if (!dragging) {
      if (Math.hypot(e.clientX - startX, e.clientY - startY) < THRESHOLD) return;
      dragging = true;
      onDragging(true);
      document.body.style.cursor = "grabbing";
    }
    place();
    if (!frame) frame = requestAnimationFrame(tick);
  };

  const finish = (drop: boolean) => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", cancel);
    window.removeEventListener("keydown", key, true);
    if (frame) cancelAnimationFrame(frame);
    line.remove();
    document.body.style.cursor = "";
    if (!dragging) {
      // A plain click on the grip selects the image.
      if (drop) editor.commands.setNodeSelection(pos);
      editor.view.focus();
      return;
    }
    onDragging(false);
    if (!drop || !target) return;
    const tr = moveImageTo(editor.state, pos, target.pos);
    if (!tr) return;
    if (src) useImageUi.getState().markSettling(src);
    editor.view.dispatch(tr.scrollIntoView());
    editor.view.focus();
  };
  const up = () => finish(true);
  const cancel = () => finish(false);
  const key = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    finish(false);
  };

  event.preventDefault();
  grip.setPointerCapture?.(event.pointerId);
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", cancel);
  window.addEventListener("keydown", key, true);
}
