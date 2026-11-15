/**
 * Voiceprints on Saved people: a person's voice folded into a few bins and drawn as a
 * spectrogram-like strip, darker where the voice is strong. Colours come from the person's
 * `speaker-N` token, so a voiceprint always reads as theirs.
 */
import type { CSSProperties, ReactNode } from "react";
import { cn } from "./cn";

/** `--c` for a speaker colour, 1..8. */
export function voiceColor(color: number): CSSProperties {
  const n = Math.min(8, Math.max(1, Math.round(color)));
  return { "--c": `var(--speaker-${n})`, "--s": `var(--speaker-${n}-soft)` } as CSSProperties;
}

/** A bin as drawn; the backend sends `null` for a value that isn't a number. */
export type VoiceBin = number | null;

const fill = (bin: VoiceBin) => {
  const v = bin ?? 0;
  return `color-mix(in oklab, var(--c) ${Math.round(10 + 90 * Math.min(1, Math.abs(v)))}%, var(--bg-sunken))`;
};

export interface VoiceBarsProps {
  bins: readonly VoiceBin[];
  /** Drawn height in px; the bars stretch to the width. */
  height: number;
  /** Sweep in from the left when mounted, after `delay` ms. */
  sweep?: boolean;
  delay?: number;
  /** Names it for screen readers; decorative without. */
  label?: string;
  className?: string;
}

/** One voiceprint as a row of bars. */
export function VoiceBars({ bins, height, sweep, delay = 0, label, className }: VoiceBarsProps) {
  const W = 400;
  const gap = 1.2;
  const n = Math.max(1, bins.length);
  const cw = (W - gap * (n - 1)) / n;
  return (
    <svg
      className={cn("nt-voice-bars", sweep && "nt-voice-sweep", className)}
      viewBox={`0 0 ${W} ${height}`}
      preserveAspectRatio="none"
      style={{ height, "--delay": `${delay}ms` } as CSSProperties}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    >
      {bins.map((v, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: bins are positional
        <rect key={i} x={i * (cw + gap)} y={0} width={cw} height={height} rx={1.5} fill={fill(v)} />
      ))}
    </svg>
  );
}

/**
 * A small picture of a voice in a list row: the voiceprint on top, the latest few samples
 * under it. Empty cells when Cilica has not learnt the voice.
 */
export function VoiceStrip({
  rows,
  color,
}: {
  rows: readonly (readonly VoiceBin[])[];
  color: number;
}) {
  const cols = 32;
  const W = 132;
  const gap = 0.6;
  const cw = (W - gap * (cols - 1)) / cols;
  const fold = (v: readonly VoiceBin[]) =>
    Array.from({ length: cols }, (_, j) => {
      const a = Math.floor((j * v.length) / cols);
      const b = Math.max(a + 1, Math.floor(((j + 1) * v.length) / cols));
      const run = v.slice(a, b);
      return run.length ? run.reduce<number>((s, x) => s + (x ?? 0), 0) / run.length : 0;
    });
  return (
    <svg className="nt-voice-strip" viewBox={`0 0 ${W} 26`} style={voiceColor(color)} aria-hidden>
      {rows.length === 0
        ? Array.from({ length: 4 * cols }, (_, k) => (
            <rect
              // biome-ignore lint/suspicious/noArrayIndexKey: cells are positional
              key={k}
              x={(k % cols) * (cw + gap)}
              y={1 + Math.floor(k / cols) * 6.2}
              width={cw}
              height={5}
              rx={0.8}
              className="nt-voice-ghost"
            />
          ))
        : rows.slice(0, 4).flatMap((row, r) =>
            fold(row).map((v, j) => (
              <rect
                // biome-ignore lint/suspicious/noArrayIndexKey: cells are positional
                key={`${r}-${j}`}
                x={j * (cw + gap)}
                y={r === 0 ? 0 : 1.5 + r * 6.1}
                width={cw}
                height={r === 0 ? 6.5 : 5}
                rx={0.8}
                fill={fill(v)}
              />
            )),
          )}
    </svg>
  );
}

/** The average voiceprint, drawn large, with what it is made from under it. */
export function VoicePanel({
  bins,
  label,
  caption,
  sweep,
}: {
  bins: readonly VoiceBin[];
  label: string;
  caption: ReactNode;
  sweep?: boolean;
}) {
  return (
    <div className="nt-voice-panel">
      <VoiceBars bins={bins} height={72} label={label} sweep={sweep} delay={60} />
      <div className="nt-voice-foot">
        <span>{caption}</span>
        <span className="nt-voice-ramp" aria-hidden>
          Weak
          <i />
          Strong
        </span>
      </div>
    </div>
  );
}
