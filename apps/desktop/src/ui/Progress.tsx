import type { CSSProperties } from "react";
import { cn } from "./cn";

/** A thin determinate progress bar. Linear, transform only. `value` is 0..1, null for unknown. */
export function Progress({
  value,
  label,
  className,
}: {
  value: number | null;
  label: string;
  className?: string;
}) {
  const v = value === null ? null : Math.min(1, Math.max(0, value));
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={v === null ? undefined : Math.round(v * 100)}
      className={cn("nt-progress", v === null && "nt-progress-indeterminate", className)}
      style={{ "--p": v ?? 0.3 } as CSSProperties}
    >
      <span />
    </div>
  );
}
