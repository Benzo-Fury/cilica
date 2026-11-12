/** Diff card (AI edits, history): insertions on `accent-soft`, deletions struck on `danger-soft`. */
import type { ReactNode } from "react";
import type { DiffRow } from "@/lib/proseDiff";
import { cn } from "./cn";

export interface DiffPart {
  value: string;
  added?: boolean;
  removed?: boolean;
}

export function DiffCard({
  head,
  foot,
  children,
  className,
  state,
}: {
  head: ReactNode;
  foot?: ReactNode;
  children: ReactNode;
  className?: string;
  /** `applied` lifts the card away (accept); `resolved` dims a card that was answered. */
  state?: "applied" | "resolved";
}) {
  return (
    <div className={cn("nt-diff", className)} data-state={state}>
      <div className="nt-diff-head">{head}</div>
      <div className="nt-diff-body">{children}</div>
      {foot && <div className="nt-diff-foot">{foot}</div>}
    </div>
  );
}

function rowParts(row: DiffRow) {
  return row.parts.map((part, i) => {
    // biome-ignore lint/suspicious/noArrayIndexKey: diff parts are positional and never reorder
    if (part.added) return <ins key={i}>{part.value}</ins>;
    // biome-ignore lint/suspicious/noArrayIndexKey: as above
    if (part.removed) return <del key={i}>{part.value}</del>;
    // biome-ignore lint/suspicious/noArrayIndexKey: as above
    return <span key={i}>{part.value}</span>;
  });
}

/**
 * A prose diff in the note's own typography: headings, list items and tasks keep their shape,
 * with insertions and deletions marked inline. No Markdown markers are shown.
 */
export function ProseDiffRows({
  rows,
  className,
  dimSame = false,
}: {
  rows: readonly DiffRow[];
  className?: string;
  /** Unchanged blocks in `ink-3` (history), so the eye lands on the change. */
  dimSame?: boolean;
}) {
  return (
    <div className={cn("nt-pd", className)}>
      {rows.map((row, i) => (
        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: rows are positional
          key={i}
          className={cn(
            "nt-pd-row",
            `nt-pd-${row.kind}`,
            row.kind === "heading" && `nt-pd-h${Math.min(3, row.level ?? 2)}`,
            dimSame && row.change === "same" && "nt-pd-same",
            row.change === "removed" && "nt-pd-removed",
          )}
          style={row.depth ? { paddingLeft: `${row.depth * 18}px` } : undefined}
        >
          {row.kind === "bullet" && (
            <span className="nt-pd-marker" aria-hidden>
              •
            </span>
          )}
          {row.kind === "ordered" && (
            <span className="nt-pd-marker nt-tnum" aria-hidden>
              {row.number}.
            </span>
          )}
          {row.kind === "task" && (
            <span
              className={cn("nt-pd-box", row.checked && "nt-pd-box-done")}
              role="img"
              aria-label={row.checked ? "Done" : "Open"}
            />
          )}
          {row.kind === "rule" ? <hr /> : <span className="nt-pd-text">{rowParts(row)}</span>}
        </div>
      ))}
    </div>
  );
}

/** Prose diff: whitespace kept, changes marked inline. */
export function DiffText({ parts, className }: { parts: readonly DiffPart[]; className?: string }) {
  return (
    <div className={cn("nt-diff-text", className)}>
      {parts.map((part, i) => {
        // biome-ignore lint/suspicious/noArrayIndexKey: diff parts are positional and never reorder
        if (part.added) return <ins key={i}>{part.value}</ins>;
        // biome-ignore lint/suspicious/noArrayIndexKey: as above
        if (part.removed) return <del key={i}>{part.value}</del>;
        // biome-ignore lint/suspicious/noArrayIndexKey: as above
        return <span key={i}>{part.value}</span>;
      })}
    </div>
  );
}
