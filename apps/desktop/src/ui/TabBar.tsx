/**
 * Open notes as tabs, along the bottom of the canvas (spec › Layout › Tabs). The bar sits on the
 * sidebar's ground, so sidebar and bar read as one frame around the page. The selected tab is part
 * of the page: the page's edge turns down into it through concave fillets (tabShape.ts).
 *
 * Motion (motion.md › Tabs):
 * - Switching: the selection travels to the new tab. Its leading edge moves on `springs.smooth`
 *   and its trailing edge on TRAIL (the same spring, 20% longer, no bounce), so it stretches a
 *   little in the direction of travel and settles as one piece.
 * - Hover: tabs get no wash. Instead the selected tab leans 4px towards the tab you point at
 *   (`springs.snappy`), showing that the selection can move there.
 * - No selected tab (Home, Graph, Settings): the tab draws back up into the page.
 * - Dragging: the tab follows the pointer 1:1 and the others slide aside (`springs.snappy`) as it
 *   passes them. A dragged selected tab carries the page's edge with it.
 * Reduce motion: the selection jumps and the others swap places without sliding; the dragged tab
 * still follows the pointer.
 */

import { Mic, X } from "lucide-react";
import {
  animate,
  type MotionValue,
  motion,
  motionValue,
  Reorder,
  useMotionValue,
  useReducedMotion,
  useTransform,
} from "motion/react";
import { type KeyboardEvent, type MouseEvent, useLayoutEffect, useRef, useState } from "react";
import { springs } from "@/app/motion";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { TAB_RADII, tabGround, tabOutline } from "./tabShape";

/** Height of the bar below the page's edge, and how far the selected tab reaches into it. */
export const TAB_BAR_HEIGHT = 44;
const TAB_DEPTH = 36;
/** How far the selected tab leans towards a hovered tab. */
const LEAN = 4;
/** The trailing edge of a moving selection: `smooth`, a little longer and without bounce. */
const TRAIL = { ...springs.smooth, visualDuration: springs.smooth.visualDuration * 1.2, bounce: 0 };

export interface TabBarItem {
  id: string;
  title: string;
  /** The note being recorded shows a red microphone. */
  recording?: boolean;
  /** A tab the next opened note will take over: its title is set in italics. */
  preview?: boolean;
}

export interface TabBarProps {
  tabs: readonly TabBarItem[];
  /** The tab on screen, or null when the canvas shows something that is not a tab. */
  activeId: string | null;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
  /** Double-click: keep a preview tab. */
  onKeep?: (id: string) => void;
  /** Dragging a tab, or ⌥⇧← / ⌥⇧→ on it, puts the tabs in a new order. */
  onReorder?: (order: string[]) => void;
  className?: string;
}

export function TabBar({
  tabs,
  activeId,
  onSelect,
  onClose,
  onKeep,
  onReorder,
  className,
}: TabBarProps) {
  const reduced = useReducedMotion();
  const listRef = useRef<HTMLDivElement>(null);
  const items = useRef(new Map<string, HTMLDivElement>());
  const [hovered, setHovered] = useState<string | null>(null);
  /** The tab being dragged; the selected tab leans towards nothing meanwhile. */
  const [dragging, setDragging] = useState<string | null>(null);
  const [width, setWidth] = useState(0);
  const previousActive = useRef<string | null>(null);

  const w = useMotionValue(0);
  const left = useMotionValue(0);
  const right = useMotionValue(0);
  const depth = useMotionValue(0);
  /** How far the selected tab is dragged from its place; the page's edge goes with it. */
  const dragX = useMotionValue(0);
  /** Every other tab's drag offset. Reorder.Item always gets one, so its hooks stay the same. */
  const offsets = useRef(new Map<string, MotionValue<number>>());
  const offsetOf = (id: string) => {
    const known = offsets.current.get(id);
    if (known) return known;
    const x = motionValue(0);
    offsets.current.set(id, x);
    return x;
  };
  const shapeOf = ([width = 0, l = 0, r = 0, d = 0, x = 0]: number[]) => ({
    width,
    left: l + x,
    right: r + x,
    depth: d,
  });
  const shape = [w, left, right, depth, dragX];
  const ground = useTransform(shape, (v: number[]) => tabGround(shapeOf(v), TAB_BAR_HEIGHT));
  const edge = useTransform(shape, (v: number[]) => tabOutline(shapeOf(v)));

  // The bar's width drives the shape; tabs reflow inside it.
  useLayoutEffect(() => {
    const el = listRef.current;
    if (!el) return;
    const measure = () => {
      setWidth(el.offsetWidth);
      w.set(el.offsetWidth);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [w]);

  const ids = tabs.map((t) => t.id);
  const order = ids.join("|");
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-measure when tabs reflow (order, width)
  useLayoutEffect(() => {
    const el = activeId ? items.current.get(activeId) : undefined;
    const switched = previousActive.current !== activeId;
    previousActive.current = activeId;

    if (!el) {
      // Nothing selected: the tab draws back up into the page where it was.
      const middle = (left.get() + right.get()) / 2;
      run(left, middle, springs.smooth);
      run(right, middle, springs.smooth);
      run(depth, 0, springs.smooth);
      return;
    }

    let a = el.offsetLeft;
    let b = a + el.offsetWidth;
    const hoveredEl =
      hovered && hovered !== activeId && !dragging ? items.current.get(hovered) : undefined;
    if (hoveredEl) {
      if (hoveredEl.offsetLeft > a) b += LEAN;
      else a = Math.max(0, a - LEAN);
    }

    // The selected tab is under the pointer: its place moved as it passed another tab, and the
    // drag offset moved back by as much, so the shape stays with the tab.
    if (reduced || dragging === activeId) {
      left.jump(a);
      right.jump(b);
      depth.jump(TAB_DEPTH);
      return;
    }

    if (depth.get() < 1) {
      // Coming out of the page: grow from the tab's middle.
      const middle = (a + b) / 2;
      left.jump(middle);
      right.jump(middle);
      run(left, a, springs.smooth);
      run(right, b, springs.smooth);
      run(depth, TAB_DEPTH, springs.smooth);
      return;
    }

    if (!switched) {
      // Leaning towards a hovered tab, or following a reflow.
      run(left, a, springs.snappy);
      run(right, b, springs.snappy);
      run(depth, TAB_DEPTH, springs.snappy);
      return;
    }

    // Travelling: the leading edge goes first, the trailing edge follows.
    const movingRight = a > left.get();
    run(left, a, movingRight ? TRAIL : springs.smooth);
    run(right, b, movingRight ? springs.smooth : TRAIL);
    run(depth, TAB_DEPTH, springs.smooth);
  }, [activeId, hovered, dragging, order, width, reduced]);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const buttons = Array.from(
      listRef.current?.querySelectorAll<HTMLButtonElement>(".nt-tab-button") ?? [],
    );
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (at < 0) return;
    // ⌥⇧← / ⌥⇧→ moves the focused tab along the bar.
    if (
      event.altKey &&
      event.shiftKey &&
      (event.key === "ArrowLeft" || event.key === "ArrowRight")
    ) {
      event.preventDefault();
      const by = event.key === "ArrowRight" ? 1 : -1;
      const ids = tabs.map((t) => t.id);
      const to = at + by;
      if (!onReorder || to < 0 || to >= ids.length) return;
      [ids[at], ids[to]] = [ids[to] as string, ids[at] as string];
      onReorder(ids);
      return;
    }
    const to =
      event.key === "ArrowRight"
        ? at + 1
        : event.key === "ArrowLeft"
          ? at - 1
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? buttons.length - 1
              : null;
    if (to === null) return;
    event.preventDefault();
    buttons[Math.max(0, Math.min(buttons.length - 1, to))]?.focus();
  };

  return (
    <motion.div
      className={cn("nt-tabbar", className)}
      initial={{ height: 0 }}
      animate={{ height: TAB_BAR_HEIGHT, transition: reduced ? { duration: 0 } : springs.smooth }}
      exit={{ height: 0, transition: reduced ? { duration: 0 } : springs.smooth }}
    >
      <svg
        className="nt-tabbar-ground"
        aria-hidden="true"
        width={width}
        height={TAB_RADII.strip + TAB_BAR_HEIGHT}
      >
        <motion.path className="nt-tabbar-fill" d={ground} fillRule="evenodd" />
        <motion.path className="nt-tabbar-edge" d={edge} />
      </svg>
      <Reorder.Group
        as="div"
        axis="x"
        values={ids}
        onReorder={(next: string[]) => onReorder?.(next)}
        ref={listRef}
        className="nt-tabbar-tabs"
        role="tablist"
        aria-label="Open notes"
        onKeyDown={onKeyDown}
        onPointerLeave={() => setHovered(null)}
      >
        {tabs.map((tab) => {
          const active = tab.id === activeId;
          return (
            <Reorder.Item
              as="div"
              key={tab.id}
              value={tab.id}
              ref={(el: HTMLDivElement | null) => {
                if (el) items.current.set(tab.id, el);
                else items.current.delete(tab.id);
              }}
              // The title button is the tab stop; the bar's own keys move between tabs.
              tabIndex={-1}
              drag={onReorder ? "x" : false}
              dragConstraints={listRef}
              dragElastic={0.08}
              style={{ x: active ? dragX : offsetOf(tab.id) }}
              transition={reduced ? { duration: 0 } : springs.snappy}
              onDragStart={() => setDragging(tab.id)}
              onDragEnd={() => setDragging(null)}
              className="nt-tab"
              data-active={active || undefined}
              data-preview={tab.preview || undefined}
              data-dragging={dragging === tab.id || undefined}
              onPointerEnter={() => setHovered(tab.id)}
              onMouseDown={(event: MouseEvent) => {
                // Middle-click closes; stop the browser's autoscroll.
                if (event.button === 1) event.preventDefault();
              }}
              onAuxClick={(event: MouseEvent) => {
                if (event.button === 1) onClose(tab.id);
              }}
            >
              <button
                type="button"
                role="tab"
                aria-selected={active}
                tabIndex={active || (!activeId && tab === tabs[0]) ? 0 : -1}
                className="nt-tab-button"
                onClick={() => onSelect(tab.id)}
                onDoubleClick={() => onKeep?.(tab.id)}
              >
                {tab.recording && <Icon icon={Mic} size="sm" className="nt-tab-rec" />}
                <span className="nt-tab-title">{tab.title}</span>
              </button>
              <button
                type="button"
                className="nt-tab-close"
                aria-label={`Close ${tab.title}`}
                tabIndex={-1}
                onClick={() => onClose(tab.id)}
              >
                <X size={12} strokeWidth={2.2} aria-hidden="true" />
              </button>
            </Reorder.Item>
          );
        })}
      </Reorder.Group>
    </motion.div>
  );
}

type Spring = (typeof springs)[keyof typeof springs] | typeof TRAIL;

function run(value: ReturnType<typeof useMotionValue<number>>, to: number, spring: Spring) {
  if (Math.abs(value.get() - to) < 0.01) return;
  animate(value, to, spring);
}
