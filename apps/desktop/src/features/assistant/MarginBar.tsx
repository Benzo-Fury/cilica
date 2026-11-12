/**
 * Margin as a floating bar at the bottom of the page (switched with the buttons in Margin's
 * header). At rest it is a pill; ⌘J or a click stretches the same shape into the composer, and the first message
 * grows it upward with the conversation, up to a cap, after which the thread scrolls. Esc, the chevron or a click on the page collapses it.
 *
 * How it opens: the open content is always laid out at its final width and anchored to the
 * bottom, so nothing reflows while the shape changes. The shape springs between measured pixel
 * sizes (never "auto"), clamped to the room the page actually has, so nothing is cut off.
 */
import { motion, useReducedMotion } from "motion/react";
import { type KeyboardEvent, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useShortcut } from "@/app/keybinds";
import { reducedTransition, springs } from "@/app/motion";
import { useRouter } from "@/app/router";
import { usePanels } from "@/features/shell/panels";
import { Kbd } from "@/ui";
import { AssistantPanel } from "./AssistantPanel";
import { useAssistantStore } from "./store";

const REST = { width: 310, height: 44 };
const OPEN_WIDTH = 560;
const THREAD_HEIGHT = 560;
/** Room kept between the bar and the edges of the page. */
const GUTTER = 16;

/** The size the open bar should take: it hugs its content, capped by THREAD_HEIGHT and the room. */
export function openSize(
  room: { width: number; height: number },
  contentHeight: number,
): { width: number; height: number } {
  const width = Math.max(REST.width, Math.min(OPEN_WIDTH, room.width - GUTTER * 2));
  const tallest = Math.min(THREAD_HEIGHT, Math.max(REST.height, room.height));
  const height = Math.min(Math.max(contentHeight, REST.height), tallest);
  return { width, height };
}

/** Track an element's box. Layout effect, so the first paint already has a size. */
function useSize<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const read = () => {
      const next = { width: el.offsetWidth, height: el.offsetHeight };
      setSize((prev) => (prev.width === next.width && prev.height === next.height ? prev : next));
    };
    read();
    const observer = new ResizeObserver(read);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return [ref, size] as const;
}

export function MarginBar() {
  const open = usePanels((s) => s.assistantOpen);
  const setOpen = usePanels((s) => s.setAssistantOpen);
  const view = useRouter((s) => s.view.name);
  const hasThread = useAssistantStore((s) => s.threadId !== null);
  const keys = useShortcut("assistant.toggle");
  const reduced = useReducedMotion();
  const barRef = useRef<HTMLDivElement>(null);
  const [dockRef, room] = useSize<HTMLDivElement>();
  const [contentRef, content] = useSize<HTMLDivElement>();
  const hidden = view === "settings";

  // A click on the page behind the bar collapses it. Menus and dialogs opened from the bar
  // live outside the canvas, so they do not count.
  useEffect(() => {
    if (!open || hidden) return;
    const onDown = (event: PointerEvent) => {
      const target = event.target as Node;
      const canvas = barRef.current?.closest(".nt-canvas");
      if (canvas?.contains(target) && !barRef.current?.contains(target)) setOpen(false);
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [open, hidden, setOpen]);

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key !== "Escape" || event.defaultPrevented) return;
    event.preventDefault();
    setOpen(false);
  };
  const label = hasThread
    ? "Back to your conversation"
    : view === "note"
      ? "Ask Margin about this note"
      : "Ask Margin about your notes";

  const target = openSize(room, content.height);
  // The content lays out at its natural height up to the cap; past it the thread scrolls.
  const maxHeight = openSize(room, Number.POSITIVE_INFINITY).height;
  const size = open ? target : REST;

  return (
    <div className="nt-marginbar-dock" ref={dockRef} hidden={hidden}>
      <motion.div
        ref={barRef}
        className="nt-marginbar"
        data-open={open || undefined}
        initial={false}
        animate={{ width: size.width, height: size.height }}
        transition={reduced ? reducedTransition : springs.smooth}
        onKeyDown={onKeyDown}
      >
        <button
          type="button"
          className="nt-marginbar-rest"
          onClick={() => setOpen(true)}
          aria-keyshortcuts={keys.join("+")}
          aria-hidden={open}
          tabIndex={open ? -1 : 0}
        >
          <span className="nt-marginbar-label">{label}</span>
          <Kbd keys={keys} />
        </button>
        <div
          ref={contentRef}
          className="nt-marginbar-open"
          style={{ width: target.width, maxHeight }}
          inert={!open}
        >
          <AssistantPanel variant="bar" />
        </div>
      </motion.div>
    </div>
  );
}
