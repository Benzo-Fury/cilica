import { cn } from "./cn";

export type StatusTone = "success" | "warning" | "danger" | "neutral" | "running";

/** A small status dot. Never colour alone: pair it with text or a label. */
export function StatusDot({ tone, label }: { tone: StatusTone; label: string }) {
  return (
    <span
      className={cn("nt-status-dot", `nt-status-${tone}`)}
      role="img"
      aria-label={label}
      title={label}
    />
  );
}
