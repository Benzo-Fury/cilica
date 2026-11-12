/**
 * The window frame: window material around the edges, panels that spring open and closed and
 * push the canvas (never overlay it), and the opaque inset canvas sheet in the middle.
 */
import { motion, useReducedMotion } from "motion/react";
import { type HTMLAttributes, type PointerEvent, type ReactNode, type Ref, useRef } from "react";
import { reducedTransition, springs } from "@/app/motion";
import { cn } from "./cn";
import { useSquircleClip } from "./Squircle";

export function WindowFrame({ children, className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cn("nt-window", className)} {...props}>
      {children}
    </div>
  );
}

export interface SidePanelProps {
  side: "left" | "right";
  open: boolean;
  width: number;
  /** Width changes from a drag apply instantly; everything else springs. */
  resizing?: boolean;
  label: string;
  children: ReactNode;
  className?: string;
}

/**
 * A panel column. The column's width springs (`springs.smooth`, interruptible); the panel keeps
 * its width and is anchored to the canvas side, so its content slides rather than squashes.
 */
export function SidePanel({
  side,
  open,
  width,
  resizing = false,
  label,
  children,
  className,
}: SidePanelProps) {
  const reduced = useReducedMotion();
  return (
    <motion.div
      className={cn("nt-panel-col", side === "left" ? "nt-panel-col-left" : "nt-panel-col-right")}
      initial={false}
      animate={{ width: open ? width : 0 }}
      transition={resizing ? { duration: 0 } : reduced ? reducedTransition : springs.smooth}
      inert={!open}
    >
      {/* A layout root: the selection pill measures against the panel, so its slide never moves it. */}
      <motion.aside
        layoutRoot
        aria-label={label}
        aria-hidden={!open}
        className={cn("nt-panel", side === "left" ? "nt-panel-left" : "nt-panel-right", className)}
        style={{ width }}
      >
        {children}
      </motion.aside>
    </motion.div>
  );
}

export interface ResizeHandleProps {
  side: "left" | "right";
  label: string;
  width: number;
  min: number;
  max: number;
  /** Snaps back to this width when released within 12px of it. */
  snapTo: number;
  onResize: (width: number) => void;
  onResizeStart?: () => void;
  onResizeEnd?: (width: number) => void;
}

/** A thin draggable edge. Tracks the pointer 1:1; snaps to the default within 12px. */
export function ResizeHandle({
  side,
  label,
  width,
  min,
  max,
  snapTo,
  onResize,
  onResizeStart,
  onResizeEnd,
}: ResizeHandleProps) {
  const start = useRef<{ x: number; width: number } | null>(null);
  const clamp = (w: number) => Math.min(max, Math.max(min, w));
  const next = (event: PointerEvent) => {
    if (!start.current) return width;
    const dx = event.clientX - start.current.x;
    return clamp(start.current.width + (side === "left" ? dx : -dx));
  };
  return (
    // biome-ignore lint/a11y/useSemanticElements: a pointer-only separator with keyboard support
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={Math.round(width)}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      className={cn("nt-resize", side === "left" ? "nt-resize-left" : "nt-resize-right")}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        start.current = { x: event.clientX, width };
        onResizeStart?.();
      }}
      onPointerMove={(event) => {
        if (start.current) onResize(next(event));
      }}
      onPointerUp={(event) => {
        if (!start.current) return;
        let w = next(event);
        if (Math.abs(w - snapTo) <= 12) w = snapTo;
        start.current = null;
        onResize(w);
        onResizeEnd?.(w);
      }}
      onDoubleClick={() => {
        onResize(snapTo);
        onResizeEnd?.(snapTo);
      }}
      onKeyDown={(event) => {
        const step = event.shiftKey ? 32 : 8;
        const dir = event.key === "ArrowLeft" ? -1 : event.key === "ArrowRight" ? 1 : 0;
        if (!dir) return;
        event.preventDefault();
        const w = clamp(width + (side === "left" ? dir : -dir) * step);
        onResize(w);
        onResizeEnd?.(w);
      }}
    />
  );
}

export interface CanvasSheetProps extends HTMLAttributes<HTMLElement> {
  children: ReactNode;
  /** Rendered in the gutters around the sheet (resize handles). */
  gutter?: ReactNode;
  ref?: Ref<HTMLElement>;
}

/** The opaque content sheet (`bg-canvas`), inset with squircle corners on macOS. */
export function CanvasSheet({ children, gutter, className, ...props }: CanvasSheetProps) {
  const ref = useRef<HTMLElement>(null);
  useSquircleClip(ref, 14);
  return (
    <div className="nt-canvas-col">
      {gutter}
      <div className="nt-canvas-shadow">
        <main ref={ref} className={cn("nt-canvas", className)} {...props}>
          {children}
        </main>
      </div>
    </div>
  );
}

/** A scrolling region inside the canvas. */
export function ScrollArea({
  children,
  className,
  ref,
  ...props
}: HTMLAttributes<HTMLDivElement> & { ref?: Ref<HTMLDivElement> }) {
  return (
    <div ref={ref} className={cn("nt-scroll", className)} {...props}>
      {children}
    </div>
  );
}
