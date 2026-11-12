import type { ReactNode } from "react";
import { cn } from "./cn";
import { Kbd } from "./Kbd";

export type EmptyArt =
  | "note"
  | "folder"
  | "trash"
  | "search"
  | "transcript"
  | "people"
  | "tasks"
  | "activity";

/** 64px line drawings in `ink-3` that draw themselves once (staggered strokes). */
const ART: Record<EmptyArt, ReactNode> = {
  note: (
    <>
      <rect x="14" y="8" width="36" height="48" rx="8" />
      <line x1="22" y1="22" x2="42" y2="22" />
      <line x1="22" y1="30" x2="38" y2="30" />
      <path d="M22 42c3-4 5 4 8 0s5 4 8 0 4 2 6 0" />
    </>
  ),
  folder: (
    <>
      <path d="M8 20a6 6 0 0 1 6-6h10l5 5h21a6 6 0 0 1 6 6v21a6 6 0 0 1-6 6H14a6 6 0 0 1-6-6Z" />
      <line x1="8" y1="27" x2="56" y2="27" />
      <line x1="26" y1="38" x2="38" y2="38" />
    </>
  ),
  trash: (
    <>
      <path d="M14 18h36" />
      <path d="M46 18l-2 32a6 6 0 0 1-6 6H26a6 6 0 0 1-6-6l-2-32" />
      <path d="M26 18v-4a4 4 0 0 1 4-4h4a4 4 0 0 1 4 4v4" />
      <path d="M28 30v14M36 30v14" />
    </>
  ),
  search: (
    <>
      <circle cx="28" cy="28" r="16" />
      <path d="M40 40l14 14" />
      <path d="M21 28h14" />
    </>
  ),
  transcript: (
    <>
      <path d="M10 26v12M18 18v28M26 12v40M34 22v20M42 16v32M50 26v12" />
      <circle cx="54" cy="12" r="0.5" />
    </>
  ),
  people: (
    <>
      <circle cx="24" cy="22" r="9" />
      <path d="M8 52c0-9 7-15 16-15s16 6 16 15" />
      <circle cx="44" cy="20" r="7" />
      <path d="M44 33c7 0 12 5 12 12" />
    </>
  ),
  tasks: (
    <>
      <rect x="10" y="12" width="44" height="42" rx="8" />
      <path d="M10 24h44" />
      <path d="M22 8v8M42 8v8" />
      <path d="M24 38l5 5 11-11" />
    </>
  ),
  activity: (
    <>
      <path d="M8 34h10l6-16 10 30 6-14h16" />
      <circle cx="32" cy="32" r="26" />
    </>
  ),
};

export interface EmptyStateProps {
  /** `false` leaves the drawing out, for screens where the words carry it alone. */
  art?: EmptyArt | false;
  /** Optional when the body says it all (Home's line from the week). */
  title?: string;
  children?: ReactNode;
  /** One primary action with its shortcut and an optional ghost alternative. */
  actions?: ReactNode;
  /** One thing worth learning while there is nothing else to read (see `EmptyTip`). */
  tip?: ReactNode;
  className?: string;
}

/** A single trick with its shortcut, shown quietly under an empty screen. */
export function EmptyTip({ keys, children }: { keys: string[]; children: ReactNode }) {
  return (
    <span className="nt-empty-tip">
      {children}
      <Kbd keys={keys} />
    </span>
  );
}

/** What will appear here, and the one action that fills it. */
export function EmptyState({
  art = "note",
  title,
  children,
  actions,
  tip,
  className,
}: EmptyStateProps) {
  return (
    <div className={cn("nt-empty", className)}>
      {art && (
        <svg
          className="nt-empty-art"
          width="64"
          height="64"
          viewBox="0 0 64 64"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.75"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden
        >
          {ART[art]}
        </svg>
      )}
      {title && <h3>{title}</h3>}
      {children && <p>{children}</p>}
      {actions && <div className="nt-empty-actions">{actions}</div>}
      {tip}
    </div>
  );
}
