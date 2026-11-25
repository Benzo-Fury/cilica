/**
 * An image in a note: the picture at its width and placement, a grip to move it, edges and
 * corners that resize it when selected (keeping its shape), and an optional caption under it. While it loads it holds
 * its shape with a skeleton; when the file can't be shown it becomes a quiet card with actions.
 *
 * Interaction only: the caller saves sizes and captions and owns every action.
 */
import { GripVertical, ImageOff } from "lucide-react";
import {
  type KeyboardEvent,
  type ReactNode,
  type PointerEvent as ReactPointerEvent,
  useEffect,
  useRef,
  useState,
} from "react";
import { cn } from "./cn";
import { Icon } from "./Icon";

export type ImageBlockAlign = "left" | "center" | "right";
export type ImageBlockState = "loading" | "ready" | "missing";

export interface ImageBlockProps {
  /** A URL the webview can load, or null while it is being worked out. */
  src: string | null;
  alt: string;
  caption: string;
  /** Width in px; null shows the image at its own size, capped to the column. */
  width: number | null;
  align: ImageBlockAlign;
  selected: boolean;
  /** Being saved or downloaded: shown dimmed with a shimmer. */
  busy?: boolean;
  /** Just dropped after a move: plays the settle spring once. */
  settling?: boolean;
  /** Being moved with the grip. */
  dragging?: boolean;
  editable: boolean;
  /** Show the caption field (empty) so a caption can be typed. */
  editingCaption: boolean;
  /** Smallest width a drag can reach. */
  minWidth?: number;
  /** What the missing card says and offers. */
  missing?: { title: string; detail: string; actions: ReactNode };
  onLoadChange?: (state: ImageBlockState, natural?: { width: number; height: number }) => void;
  /** Live while dragging a handle. */
  onResize?: (width: number) => void;
  onResizeEnd?: (width: number) => void;
  onCaptionCommit?: (caption: string) => void;
  onCaptionDone?: () => void;
  onOpen?: () => void;
  onGripPointerDown?: (event: ReactPointerEvent<HTMLButtonElement>) => void;
  /** Keyboard on the grip: ↑/↓ move the image. */
  onGripKeyDown?: (event: KeyboardEvent<HTMLButtonElement>) => void;
}

/** Where a resize starts: one of the four edges or four corners of the picture. */
type Edge = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";
const EDGES: Edge[] = ["n", "s", "e", "w", "ne", "nw", "se", "sw"];

export function ImageBlock({
  src,
  alt,
  caption,
  width,
  align,
  selected,
  busy = false,
  settling = false,
  dragging = false,
  editable,
  editingCaption,
  minWidth = 48,
  missing,
  onLoadChange,
  onResize,
  onResizeEnd,
  onCaptionCommit,
  onCaptionDone,
  onOpen,
  onGripPointerDown,
  onGripKeyDown,
}: ImageBlockProps) {
  const frame = useRef<HTMLDivElement>(null);
  const img = useRef<HTMLImageElement>(null);
  const [state, setState] = useState<ImageBlockState>("loading");
  const [liveWidth, setLiveWidth] = useState<number | null>(null);

  // A new source starts loading again, unless the browser already has it (a cached image can
  // finish before this runs, so its load event has come and gone).
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset per source.
  useEffect(() => {
    const el = img.current;
    if (src && el?.complete && el.naturalWidth > 0) {
      setState("ready");
      onLoadChange?.("ready", { width: el.naturalWidth, height: el.naturalHeight });
      return;
    }
    setState(src ? "loading" : missing ? "missing" : "loading");
  }, [src]);

  const report = (next: ImageBlockState, natural?: { width: number; height: number }) => {
    setState(next);
    onLoadChange?.(next, natural);
  };

  // Edges and corners: the opposite side stays put and the shape is kept, so every drag only
  // changes the width. A centred image grows from both sides, so its sideways drags count twice.
  const startResize = (edge: Edge) => (event: ReactPointerEvent<HTMLSpanElement>) => {
    const el = frame.current;
    const column = el?.parentElement?.clientWidth ?? 0;
    if (!el || event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    // Layout sizes, not on-screen ones: the selected picture is drawn slightly scaled up.
    const startX = event.clientX;
    const startY = event.clientY;
    const startWidth = el.offsetWidth;
    const ratio = el.offsetHeight > 0 ? el.offsetWidth / el.offsetHeight : 1;
    const factor = align === "center" ? 2 : 1;
    const sx = edge.includes("e") ? 1 : edge.includes("w") ? -1 : 0;
    const sy = edge.includes("s") ? 1 : edge.includes("n") ? -1 : 0;
    let current = startWidth;
    const target = event.currentTarget;
    target.setPointerCapture(event.pointerId);
    const move = (e: PointerEvent) => {
      const byX = sx * factor * (e.clientX - startX);
      const byY = sy * ratio * (e.clientY - startY);
      // A corner follows whichever way the pointer moved further.
      const delta = sx && sy ? (Math.abs(byX) >= Math.abs(byY) ? byX : byY) : sx ? byX : byY;
      const next = startWidth + delta;
      current = Math.round(Math.min(Math.max(next, minWidth), column || next));
      setLiveWidth(current);
      onResize?.(current);
    };
    const up = () => {
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerup", up);
      target.removeEventListener("pointercancel", up);
      setLiveWidth(null);
      if (Math.abs(current - startWidth) >= 1) onResizeEnd?.(current);
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerup", up);
    target.addEventListener("pointercancel", up);
  };

  const shownWidth = liveWidth ?? width;
  const isMissing = state === "missing" || (!src && !!missing);
  const showCaption = !isMissing && (caption.trim() !== "" || editingCaption);

  return (
    <figure
      className="nt-image"
      data-align={align}
      data-selected={selected ? "" : undefined}
      data-state={isMissing ? "missing" : state}
      data-busy={busy ? "" : undefined}
      data-settling={settling ? "" : undefined}
      data-dragging={dragging ? "" : undefined}
      data-resizing={liveWidth !== null ? "" : undefined}
    >
      {/* A layer over the picture takes the pointer (see images.css), so the double-click is the frame's. */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: double-click opens the viewer; the toolbar and menus offer it too. */}
      <div
        ref={frame}
        className="nt-image-frame"
        style={shownWidth && !isMissing ? { width: shownWidth } : undefined}
        onDoubleClick={isMissing ? undefined : onOpen}
      >
        {editable && (
          <button
            type="button"
            className="nt-image-grip"
            aria-label="Move image"
            title="Drag to move, or use ⌥↑ and ⌥↓"
            contentEditable={false}
            onPointerDown={onGripPointerDown}
            onKeyDown={onGripKeyDown}
          >
            <Icon icon={GripVertical} size="sm" />
          </button>
        )}
        {isMissing && missing ? (
          <div className="nt-image-missing">
            <Icon icon={ImageOff} />
            <div className="nt-image-missing-text">
              <p className="nt-image-missing-title">{missing.title}</p>
              <p className="nt-image-missing-detail">{missing.detail}</p>
            </div>
            <div className="nt-image-missing-actions">{missing.actions}</div>
          </div>
        ) : (
          <>
            {src && (
              <img
                ref={img}
                src={src}
                alt={alt}
                draggable={false}
                className={cn("nt-image-img", state !== "ready" && "nt-image-img-loading")}
                onLoad={(e) =>
                  report("ready", {
                    width: e.currentTarget.naturalWidth,
                    height: e.currentTarget.naturalHeight,
                  })
                }
                onError={() => report("missing")}
              />
            )}
            {(state === "loading" || !src) && (
              <span className="nt-image-skel nt-skel" aria-hidden />
            )}
            {editable &&
              selected &&
              state === "ready" &&
              EDGES.map((edge) => (
                <span
                  key={edge}
                  className="nt-image-edge"
                  data-edge={edge}
                  aria-hidden
                  onPointerDown={startResize(edge)}
                />
              ))}
            {liveWidth !== null && (
              <span className="nt-image-size" aria-hidden>
                {liveWidth} px
              </span>
            )}
          </>
        )}
      </div>
      {showCaption && (
        <figcaption
          className="nt-image-caption"
          style={shownWidth ? { maxWidth: shownWidth } : undefined}
        >
          {editable ? (
            <CaptionField
              value={caption}
              autoFocus={editingCaption}
              onCommit={(value) => onCaptionCommit?.(value)}
              onDone={() => onCaptionDone?.()}
            />
          ) : (
            caption
          )}
        </figcaption>
      )}
    </figure>
  );
}

/** The caption, edited in place. Enter or leaving the field saves; Esc puts it back. */
function CaptionField({
  value,
  autoFocus,
  onCommit,
  onDone,
}: {
  value: string;
  autoFocus: boolean;
  onCommit: (value: string) => void;
  onDone: () => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const cancelled = useRef(false);
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  useEffect(() => {
    if (autoFocus) input.current?.focus();
  }, [autoFocus]);
  return (
    <input
      ref={input}
      className="nt-image-caption-input"
      value={draft}
      placeholder="Add a caption"
      aria-label="Caption"
      onChange={(e) => setDraft(e.target.value)}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") {
          e.preventDefault();
          input.current?.blur();
        } else if (e.key === "Escape") {
          e.preventDefault();
          cancelled.current = true;
          input.current?.blur();
        }
      }}
      onBlur={() => {
        if (cancelled.current) setDraft(value);
        else if (draft.trim() !== value.trim()) onCommit(draft.trim());
        cancelled.current = false;
        onDone();
      }}
    />
  );
}
