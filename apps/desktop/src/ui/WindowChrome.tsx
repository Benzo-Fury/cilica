/**
 * Window chrome: the 52px title band. macOS keeps its real traffic lights (inset 18px, centred
 * in the band); Windows and Linux get Margin's own minimise / maximise / close buttons.
 * Empty band areas carry `data-tauri-drag-region`. Double-click does what the system setting asks:
 * on macOS through `useTitleBarDoubleClick` (native animation), elsewhere Tauri maximises.
 */
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

export interface TitleBandProps extends HTMLAttributes<HTMLDivElement> {
  children?: ReactNode;
  /** The canvas band has its own padding; every band is the title bar's height. */
  height?: "window" | "canvas";
}

/** A draggable band. Interactive children stay clickable (Tauri ignores drags that start on them). */
export function TitleBand({ children, className, height = "window", ...props }: TitleBandProps) {
  return (
    <div
      data-tauri-drag-region
      className={cn("nt-band", height === "canvas" && "nt-band-canvas", className)}
      {...props}
    >
      {children}
    </div>
  );
}

/** Reserves the space macOS's traffic lights occupy at the start of a band. */
export function TrafficLightsSpace({ preview = false }: { preview?: boolean }) {
  return (
    <span className="nt-lights" data-tauri-drag-region aria-hidden>
      {preview && (
        <>
          <i className="nt-light nt-light-close" />
          <i className="nt-light nt-light-min" />
          <i className="nt-light nt-light-max" />
        </>
      )}
    </span>
  );
}

export interface WindowControlsProps {
  /** Left out for windows that cannot be minimised or resized (setup). */
  onMinimize?: () => void;
  onToggleMaximize?: () => void;
  onClose: () => void;
  className?: string;
}

/** Windows and Linux caption buttons: 46px wide, 10px glyphs; Close turns `danger` on hover. */
export function WindowControls({
  onMinimize,
  onToggleMaximize,
  onClose,
  className,
}: WindowControlsProps) {
  return (
    <div className={cn("nt-win-caps", className)}>
      {onMinimize && (
        <button type="button" aria-label="Minimise" onClick={onMinimize}>
          <svg viewBox="0 0 10 10" aria-hidden>
            <path d="M1 5h8" stroke="currentColor" strokeWidth="1" />
          </svg>
        </button>
      )}
      {onToggleMaximize && (
        <button type="button" aria-label="Maximise" onClick={onToggleMaximize}>
          <svg viewBox="0 0 10 10" aria-hidden>
            <rect
              x="1.5"
              y="1.5"
              width="7"
              height="7"
              rx="1.5"
              fill="none"
              stroke="currentColor"
              strokeWidth="1"
            />
          </svg>
        </button>
      )}
      <button type="button" className="close" aria-label="Close" onClick={onClose}>
        <svg viewBox="0 0 10 10" aria-hidden>
          <path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>
    </div>
  );
}
