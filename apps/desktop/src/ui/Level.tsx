import type { CSSProperties } from "react";

/** Fixed per-bar weights so the meter reads as a voice shape, not a flat block. */
const SHAPES: Record<number, readonly number[]> = {
  5: [0.55, 0.85, 1, 0.75, 0.5],
  12: [0.4, 0.62, 0.85, 0.7, 1, 0.78, 0.92, 0.66, 0.88, 0.58, 0.74, 0.46],
};

function shape(bars: number): readonly number[] {
  return (
    SHAPES[bars] ??
    Array.from({ length: bars }, (_, i) => 0.55 + 0.45 * Math.sin((i + 1) * 1.7) ** 2)
  );
}

/**
 * A live level meter: bars follow the input level (0..1) at about 10fps with a 90ms linear
 * height change. `jitter` (0..1 per bar) keeps it alive between level updates.
 */
export function LevelMeter({
  level,
  bars = 12,
  jitter,
  className,
  label,
}: {
  level: number;
  bars?: 5 | 12;
  jitter?: readonly number[];
  className?: string;
  /** Accessible description; the meter is decorative without it. */
  label?: string;
}) {
  const weights = shape(bars);
  return (
    <span
      className={className ? `nt-wave ${className}` : "nt-wave"}
      {...(label
        ? {
            role: "meter",
            "aria-label": label,
            "aria-valuemin": 0,
            "aria-valuemax": 100,
            "aria-valuenow": Math.round(level * 100),
          }
        : { "aria-hidden": true })}
    >
      {weights.map((w, i) => {
        const wobble = jitter?.[i % (jitter.length || 1)] ?? 1;
        const h = Math.max(0.12, Math.min(1, level * 1.35 * w * (0.75 + 0.25 * wobble)));
        // biome-ignore lint/suspicious/noArrayIndexKey: bars are positional and never reorder
        return <i key={i} style={{ "--h": `${Math.round(h * 100)}%` } as CSSProperties} />;
      })}
    </span>
  );
}
