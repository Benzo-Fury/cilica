import type { ReactNode } from "react";
import { cn } from "./cn";
import { TitleBand } from "./WindowChrome";

/**
 * The canvas title band as three columns: start (crumb), a centred control, end (actions).
 * `trail` sits in the end column right after the centred control (the recording control, the
 * recording's player), so it can grow, shrink or go without moving the centred control.
 */
export function CanvasBandLayout({
  start,
  center,
  trail,
  end,
  className,
}: {
  start?: ReactNode;
  center?: ReactNode;
  trail?: ReactNode;
  end?: ReactNode;
  className?: string;
}) {
  return (
    <TitleBand height="canvas" className={cn("nt-band-grid", className)}>
      <div className="nt-band-start" data-tauri-drag-region>
        {start}
      </div>
      <div className="nt-band-center">{center}</div>
      <div className="nt-band-end" data-tauri-drag-region>
        {trail && <div className="nt-band-trail">{trail}</div>}
        {end}
      </div>
    </TitleBand>
  );
}

/** A view inside the canvas; views overlap while one fades out and the next slides in. */
export function ViewLayer({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("nt-view", className)}>{children}</div>;
}
