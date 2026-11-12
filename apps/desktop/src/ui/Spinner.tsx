import { cn } from "./cn";

/** A 14px spinner for loading controls. Prefer shimmer text for Margin's work. */
export function Spinner({ className, label }: { className?: string; label?: string }) {
  if (label) {
    return (
      <span role="status" className={cn("nt-spinner", className)}>
        <span className="nt-sr-only">{label}</span>
      </span>
    );
  }
  return <span aria-hidden className={cn("nt-spinner", className)} />;
}
