/**
 * The full-window image viewer: one of a note's images at a time over a dark scrim.
 * Pinch or ⌘-scroll zooms at the pointer, scroll or drag pans a zoomed image, double-click
 * toggles fit and 2×, ←/→ step through the note's images, + / − / 0 zoom, Esc closes.
 * It opens from the image with `springs.smooth` and crossfades under Reduce motion.
 */
import { ChevronLeft, ChevronRight, Maximize2, X, ZoomIn, ZoomOut } from "lucide-react";
import { AnimatePresence, animate, motion, useMotionValue, useReducedMotion } from "motion/react";
import { Dialog as D } from "radix-ui";
import {
  type PointerEvent as ReactPointerEvent,
  useEffect,
  useRef,
  useState,
  type WheelEvent,
} from "react";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { IconButton } from "./IconButton";

export interface ViewerImage {
  src: string;
  alt: string;
  caption: string;
}

const MIN_ZOOM = 1;
const MAX_ZOOM = 8;
const clamp = (n: number, lo: number, hi: number) => Math.min(Math.max(n, lo), hi);

export function ImageViewer({
  images,
  index,
  open,
  onIndexChange,
  onOpenChange,
}: {
  images: ViewerImage[];
  index: number;
  open: boolean;
  onIndexChange: (index: number) => void;
  onOpenChange: (open: boolean) => void;
}) {
  const reduced = useReducedMotion() ?? false;
  const image = images[index];
  const count = images.length;
  const stage = useRef<HTMLDivElement>(null);
  const scale = useMotionValue(1);
  const x = useMotionValue(0);
  const y = useMotionValue(0);
  const [zoomed, setZoomed] = useState(false);

  const travel = reduced ? reducedTransition : springs.snappy;

  /** Zoom to `next` keeping the point under (cx, cy), relative to the stage centre, still. */
  const zoomTo = (next: number, cx = 0, cy = 0, animated = true) => {
    const from = scale.get();
    const to = clamp(next, MIN_ZOOM, MAX_ZOOM);
    const ratio = to / from;
    const nx = to === 1 ? 0 : cx - (cx - x.get()) * ratio;
    const ny = to === 1 ? 0 : cy - (cy - y.get()) * ratio;
    setZoomed(to > 1.01);
    if (animated) {
      void animate(scale, to, travel);
      void animate(x, nx, travel);
      void animate(y, ny, travel);
    } else {
      scale.set(to);
      x.set(nx);
      y.set(ny);
    }
  };

  // Each image opens fitted.
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset per image.
  useEffect(() => {
    scale.set(1);
    x.set(0);
    y.set(0);
    setZoomed(false);
  }, [index, open]);

  const step = (delta: number) => {
    if (count < 2) return;
    onIndexChange((index + delta + count) % count);
  };

  const centreOf = (clientX: number, clientY: number) => {
    const r = stage.current?.getBoundingClientRect();
    if (!r) return { cx: 0, cy: 0 };
    return { cx: clientX - (r.left + r.width / 2), cy: clientY - (r.top + r.height / 2) };
  };

  const onWheel = (event: WheelEvent<HTMLDivElement>) => {
    // A trackpad pinch arrives as a wheel with ctrlKey.
    if (event.ctrlKey || event.metaKey) {
      const { cx, cy } = centreOf(event.clientX, event.clientY);
      zoomTo(scale.get() * Math.exp(-event.deltaY * 0.01), cx, cy, false);
      return;
    }
    if (scale.get() > 1) {
      x.set(x.get() - event.deltaX);
      y.set(y.get() - event.deltaY);
    }
  };

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || scale.get() <= 1) return;
    const el = event.currentTarget;
    el.setPointerCapture(event.pointerId);
    const start = { px: event.clientX, py: event.clientY, x: x.get(), y: y.get() };
    const move = (e: PointerEvent) => {
      x.set(start.x + e.clientX - start.px);
      y.set(start.y + e.clientY - start.py);
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "ArrowLeft") step(-1);
    else if (event.key === "ArrowRight") step(1);
    else if (event.key === "+" || event.key === "=") zoomTo(scale.get() * 1.5);
    else if (event.key === "-") zoomTo(scale.get() / 1.5);
    else if (event.key === "0") zoomTo(1);
    else return;
    event.preventDefault();
  };

  return (
    <D.Root open={open} onOpenChange={onOpenChange}>
      <AnimatePresence>
        {open && image && (
          <D.Portal forceMount>
            <D.Overlay asChild forceMount>
              <motion.div
                className="nt-viewer-scrim"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: tweens.base }}
                exit={{ opacity: 0, transition: tweens.exit }}
              />
            </D.Overlay>
            <D.Content
              forceMount
              className="nt-viewer"
              aria-describedby={undefined}
              onKeyDown={onKeyDown}
              // Focus the viewer itself, so no button's tooltip shows as it opens.
              onOpenAutoFocus={(event) => {
                event.preventDefault();
                (event.currentTarget as HTMLElement | null)?.focus();
              }}
              tabIndex={-1}
            >
              <D.Title className="nt-sr-only">{image.alt || "Image"}</D.Title>
              <motion.div
                className="nt-viewer-body"
                initial={reduced ? { opacity: 0 } : { opacity: 0, scale: 0.96 }}
                animate={{
                  opacity: 1,
                  scale: 1,
                  transition: reduced ? reducedTransition : springs.smooth,
                }}
                exit={{ opacity: 0, scale: reduced ? 1 : 0.98, transition: tweens.exit }}
              >
                {/* biome-ignore lint/a11y/noStaticElementInteractions: pointer zoom and pan; the keys (+ − 0 ← → Esc) live on the dialog. */}
                {/* biome-ignore lint/a11y/useKeyWithClickEvents: a click outside the picture closes, like Esc. */}
                <div
                  ref={stage}
                  className="nt-viewer-stage"
                  data-zoomed={zoomed ? "" : undefined}
                  onWheel={onWheel}
                  onPointerDown={onPointerDown}
                  onDoubleClick={(e) => {
                    const { cx, cy } = centreOf(e.clientX, e.clientY);
                    zoomTo(scale.get() > 1.01 ? 1 : 2, cx, cy);
                  }}
                  onClick={(e) => {
                    // A click on the empty stage (not the picture) closes, like the scrim.
                    if (e.target === e.currentTarget && !zoomed) onOpenChange(false);
                  }}
                >
                  <AnimatePresence mode="popLayout" initial={false}>
                    <motion.img
                      key={image.src}
                      src={image.src}
                      alt={image.alt}
                      draggable={false}
                      className="nt-viewer-img"
                      style={{ scale, x, y }}
                      initial={{ opacity: 0 }}
                      animate={{ opacity: 1, transition: tweens.base }}
                      exit={{ opacity: 0, transition: tweens.exit }}
                    />
                  </AnimatePresence>
                </div>
                <div className="nt-viewer-bar nt-glass-strong">
                  {count > 1 && (
                    <>
                      <IconButton
                        icon={ChevronLeft}
                        label="Previous image"
                        shortcut={["←"]}
                        tooltipSide="top"
                        onClick={() => step(-1)}
                      />
                      <span className="nt-viewer-count">
                        {index + 1} of {count}
                      </span>
                      <IconButton
                        icon={ChevronRight}
                        label="Next image"
                        shortcut={["→"]}
                        tooltipSide="top"
                        onClick={() => step(1)}
                      />
                      <span className="nt-formatbar-divider" aria-hidden />
                    </>
                  )}
                  <IconButton
                    icon={ZoomOut}
                    label="Zoom out"
                    shortcut={["−"]}
                    tooltipSide="top"
                    onClick={() => zoomTo(scale.get() / 1.5)}
                  />
                  <IconButton
                    icon={Maximize2}
                    label="Fit to window"
                    shortcut={["0"]}
                    tooltipSide="top"
                    disabled={!zoomed}
                    onClick={() => zoomTo(1)}
                  />
                  <IconButton
                    icon={ZoomIn}
                    label="Zoom in"
                    shortcut={["+"]}
                    tooltipSide="top"
                    onClick={() => zoomTo(scale.get() * 1.5)}
                  />
                  <span className="nt-formatbar-divider" aria-hidden />
                  <D.Close asChild>
                    <IconButton icon={X} label="Close" shortcut={["Esc"]} tooltipSide="top" />
                  </D.Close>
                </div>
                {image.caption && <p className="nt-viewer-caption">{image.caption}</p>}
              </motion.div>
            </D.Content>
          </D.Portal>
        )}
      </AnimatePresence>
    </D.Root>
  );
}
