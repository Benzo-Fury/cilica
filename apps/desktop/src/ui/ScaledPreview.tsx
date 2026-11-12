/**
 * A live miniature: renders its children at a fixed full size and scales them to fit the width it
 * is given. The copy is inert, so nothing in it takes focus or clicks.
 */
import { type CSSProperties, type ReactNode, useEffect, useRef, useState } from "react";

export function ScaledPreview({
  children,
  width = 720,
  height = 420,
  caption,
  label,
}: {
  children: ReactNode;
  /** The size the children are laid out at, in CSS pixels. */
  width?: number;
  height?: number;
  /** A quiet line under the frame. */
  caption?: ReactNode;
  /** What the preview shows, for assistive tech. */
  label: string;
}) {
  const frame = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(0.5);
  useEffect(() => {
    const el = frame.current;
    if (!el) return;
    const measure = () => setScale(el.clientWidth / width);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [width]);
  return (
    <figure className="nt-scaled-preview">
      <div
        ref={frame}
        className="nt-scaled-preview-frame"
        role="img"
        aria-label={label}
        style={{ aspectRatio: `${width} / ${height}` }}
      >
        <div
          className="nt-scaled-preview-stage"
          inert
          aria-hidden
          style={{ width, height, "--preview-scale": scale } as CSSProperties}
        >
          {children}
        </div>
      </div>
      {caption && <figcaption>{caption}</figcaption>}
    </figure>
  );
}
