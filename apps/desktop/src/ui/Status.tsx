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

/** Onboarding progress: one dot per step, the current one stretched. */
export function StepDots({ count, current }: { count: number; current: number }) {
  return (
    <div
      className="nt-steps"
      role="progressbar"
      aria-valuemin={1}
      aria-valuemax={count}
      aria-valuenow={current + 1}
      aria-label={`Step ${current + 1} of ${count}`}
    >
      {Array.from({ length: count }, (_, i) => (
        <span
          // biome-ignore lint/suspicious/noArrayIndexKey: steps are positional
          key={i}
          className={cn(
            "nt-step-dot",
            i === current && "nt-step-dot-current",
            i < current && "nt-step-dot-done",
          )}
        />
      ))}
    </div>
  );
}
