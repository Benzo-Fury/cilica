/**
 * The drawings Home's faces are made of: counters, bars, dials, hour dots, quotes, a doodle pad and
 * the small cards the newer faces use. Presentational only; the Home feature supplies the data.
 */
import { useReducedMotion } from "motion/react";
import {
  type CSSProperties,
  type PointerEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
import { cn } from "./cn";

/** Counts up from zero once, on a decelerating curve. Tabular figures, so nothing shifts. */
export function CountUp({ value, durationMs = 900 }: { value: number; durationMs?: number }) {
  const reduced = useReducedMotion();
  const [shown, setShown] = useState(reduced ? value : 0);
  useEffect(() => {
    if (reduced) {
      setShown(value);
      return;
    }
    let frame = 0;
    const start = performance.now();
    const step = (now: number) => {
      const t = Math.min(1, (now - start) / durationMs);
      setShown(Math.round(value * (1 - (1 - t) ** 3)));
      if (t < 1) frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [value, durationMs, reduced]);
  return (
    <span className="nt-countup">
      <span className="nt-sr-only">{value}</span>
      <span aria-hidden>{shown}</span>
    </span>
  );
}

/** A word set huge in outline; the letters under the pointer fill with the accent and lift. */
export function OutlineWord({ word, onClick }: { word: string; onClick?: () => void }) {
  const ref = useRef<HTMLButtonElement>(null);
  const [near, setNear] = useState<number[]>([]);
  const move = (x: number) => {
    const letters = ref.current?.querySelectorAll<HTMLElement>("[data-letter]");
    if (!letters) return;
    const out: number[] = [];
    letters.forEach((el, i) => {
      const r = el.getBoundingClientRect();
      const d = Math.abs(x - (r.left + r.width / 2));
      out[i] = d < 40 ? 1 - d / 40 : 0;
    });
    setNear(out);
  };
  return (
    <button
      ref={ref}
      type="button"
      className="nt-outline-word"
      onPointerMove={(e) => move(e.clientX)}
      onPointerLeave={() => setNear([])}
      onClick={onClick}
      aria-label={word}
    >
      {[...word].map((ch, i) => {
        const n = near[i] ?? 0;
        return (
          <span
            // Letters repeat ("onboarding"), so the position is the identity.
            // biome-ignore lint/suspicious/noArrayIndexKey: letters have no other identity
            key={i}
            data-letter
            data-hot={n > 0.35 || undefined}
            style={{ transform: n ? `translateY(${(-8 * n).toFixed(1)}px)` : undefined }}
            aria-hidden
          >
            {ch}
          </span>
        );
      })}
    </button>
  );
}

export interface PullQuoteProps {
  text: string;
  /** Who said it and where, under the quote. */
  byline: ReactNode;
  /** Opens the moment in the recording. */
  onOpen?: () => void;
  openLabel?: string;
}

/** A line from a meeting set as a pull quote, its words arriving the way a transcript does. */
export function PullQuote({ text, byline, onOpen, openLabel }: PullQuoteProps) {
  const words = `“${text}”`.split(" ");
  return (
    <figure className="nt-pullquote">
      <button type="button" className="nt-pullquote-text" onClick={onOpen} aria-label={openLabel}>
        <blockquote key={text}>
          {words.map((w, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: words repeat; order is the identity
            <span key={i} style={{ "--i": i } as CSSProperties}>
              {w}{" "}
            </span>
          ))}
        </blockquote>
      </button>
      <figcaption>{byline}</figcaption>
    </figure>
  );
}

interface Stroke {
  points: { x: number; y: number }[];
  /** When the pen lifted; strokes fade from then. Infinity while drawing. */
  lifted: number;
}

const FADE_MS = 3000;

/**
 * The pane as a scrap of paper. The pointer draws in the empty-state line weight and every line
 * dissolves a few seconds after the pen lifts. Nothing is kept.
 */
export function DoodlePad({ children, className }: { children: ReactNode; className?: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const strokes = useRef<Stroke[]>([]);
  const drawing = useRef<Stroke | null>(null);
  const frame = useRef(0);
  const reduced = useReducedMotion();

  useEffect(() => {
    const el = canvas.current;
    const ctx = el?.getContext("2d");
    if (!el || !ctx) return;
    const resize = () => {
      const r = el.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      el.width = Math.round(r.width * dpr);
      el.height = Math.round(r.height * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(el);
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame.current);
    };
  }, []);

  const paint = () => {
    const el = canvas.current;
    const ctx = el?.getContext("2d");
    if (!el || !ctx) return;
    const now = performance.now();
    const life = reduced ? FADE_MS / 3 : FADE_MS;
    strokes.current = strokes.current.filter((s) => now - s.lifted < life);
    ctx.clearRect(0, 0, el.width, el.height);
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.lineWidth = 1.75;
    ctx.strokeStyle = getComputedStyle(el).color;
    for (const s of strokes.current) {
      // Reduce motion: lines vanish at once instead of fading.
      ctx.globalAlpha = reduced ? 1 : Math.max(0, Math.min(1, 1 - (now - s.lifted) / life));
      ctx.beginPath();
      for (const [i, p] of s.points.entries()) {
        if (i) ctx.lineTo(p.x, p.y);
        else ctx.moveTo(p.x, p.y);
      }
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
    frame.current = strokes.current.length ? requestAnimationFrame(paint) : 0;
  };
  const wake = () => {
    if (!frame.current) frame.current = requestAnimationFrame(paint);
  };
  const point = (e: PointerEvent) => {
    const r = e.currentTarget.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };

  return (
    <div className={cn("nt-doodle", className)}>
      <canvas
        ref={canvas}
        aria-hidden
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          drawing.current = { points: [point(e)], lifted: Number.POSITIVE_INFINITY };
          strokes.current.push(drawing.current);
          wake();
        }}
        onPointerMove={(e) => {
          if (!drawing.current) return;
          drawing.current.points.push(point(e));
          wake();
        }}
        onPointerUp={() => {
          if (drawing.current) drawing.current.lifted = performance.now();
          drawing.current = null;
          wake();
        }}
        onPointerCancel={() => {
          if (drawing.current) drawing.current.lifted = performance.now();
          drawing.current = null;
        }}
      />
      <div className="nt-doodle-content">{children}</div>
    </div>
  );
}

export interface TalkBarSlice {
  id: string;
  /** 1..8: the `speaker-N` colour. None for a folded "everyone else" slice. */
  color: number | null;
  /** Relative size. */
  value: number;
  /** What the caption says while this slice is pointed at. */
  detail: string;
}

/** One bar split into slices, each in its speaker's colour. Pointing at a slice names it below. */
export function TalkBar({ slices, caption }: { slices: TalkBarSlice[]; caption: string }) {
  const [hover, setHover] = useState<TalkBarSlice | null>(null);
  return (
    <div className="nt-talkbar">
      <div className="nt-talkbar-track">
        {slices.map((s, i) => (
          <button
            type="button"
            key={s.id}
            aria-label={s.detail}
            style={
              {
                flexGrow: s.value,
                "--c": s.color ? `var(--speaker-${s.color})` : "var(--ink-4)",
                "--i": i,
              } as CSSProperties
            }
            onPointerEnter={() => setHover(s)}
            onPointerLeave={() => setHover(null)}
            onFocus={() => setHover(s)}
            onBlur={() => setHover(null)}
          />
        ))}
      </div>
      <span className="nt-talkbar-caption" aria-live="polite">
        {hover?.detail ?? caption}
      </span>
    </div>
  );
}

const GAUGE_R = 88;

/** A half dial with a needle that swings to `value`, and a faint mark at `previous`. */
export function PaceGauge({
  value,
  previous,
  min = 80,
  max = 220,
}: {
  value: number;
  previous?: number | null;
  min?: number;
  max?: number;
}) {
  const reduced = useReducedMotion();
  const deg = (v: number) => -180 + ((Math.min(max, Math.max(min, v)) - min) / (max - min)) * 180;
  const at = (v: number, r: number) => {
    const a = (deg(v) * Math.PI) / 180;
    return { x: Math.cos(a) * r, y: Math.sin(a) * r };
  };
  const target = deg(value);
  // Start at the left stop and swing to the value on the next frame, so the needle springs.
  const [angle, setAngle] = useState(reduced ? target : -180);
  useEffect(() => {
    const frame = requestAnimationFrame(() => setAngle(target));
    return () => cancelAnimationFrame(frame);
  }, [target]);
  const ticks = [min, (min + max) / 2, max];
  const last = previous ? [at(previous, GAUGE_R - 9), at(previous, GAUGE_R + 9)] : null;
  return (
    <svg className="nt-gauge" viewBox="-108 -100 216 112" aria-hidden>
      <path
        className="nt-gauge-arc"
        d={`M ${-GAUGE_R} 0 A ${GAUGE_R} ${GAUGE_R} 0 0 1 ${GAUGE_R} 0`}
      />
      {ticks.map((t) => {
        const p = at(t, GAUGE_R + 16);
        return (
          <text key={t} className="nt-gauge-label" x={p.x} y={p.y + 4} textAnchor="middle">
            {Math.round(t)}
          </text>
        );
      })}
      {last && (
        <line
          className="nt-gauge-last"
          x1={last[0]?.x}
          y1={last[0]?.y}
          x2={last[1]?.x}
          y2={last[1]?.y}
        />
      )}
      <line
        className="nt-gauge-needle"
        x1={0}
        y1={0}
        x2={GAUGE_R - 14}
        y2={0}
        style={{ transform: `rotate(${angle}deg)` }}
      />
      <circle className="nt-gauge-hub" r={4} />
    </svg>
  );
}

/** Two stretches of waveform with a hole between them, the hole labelled with its length. */
export function SilenceGap({ label }: { label: string }) {
  const bars = (seed: number) =>
    Array.from(
      { length: 22 },
      (_, i) => 6 + ((i * 7 + seed) % 11) * 3 * Math.sin(((i + 1) / 23) * Math.PI),
    );
  return (
    <div className="nt-silence" aria-hidden>
      <span className="nt-silence-wave">
        {bars(3).map((h, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed decorative bars
          <i key={i} style={{ height: `${h}px` }} />
        ))}
      </span>
      <span className="nt-silence-hole">
        <b>{label}</b>
      </span>
      <span className="nt-silence-wave">
        {bars(8).map((h, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed decorative bars
          <i key={i} style={{ height: `${h}px` }} />
        ))}
      </span>
    </div>
  );
}

/** A quiet row pinned to the bottom of the pane, above anything drawn on it. */
export function GlanceFooter({ children }: { children: ReactNode }) {
  return <div className="nt-glance-footer">{children}</div>;
}

/** Geometry of the hour-dots chart, in viewBox units. */
const DOTS_LEFT = 8;
const DOTS_COL = 16;
const DOTS_BASE = 138;
const DOTS_ROOM = 112;

/**
 * A day laid out left to right, midnight to midnight, with a column of dots for each hour: one
 * dot per `perDot` saves. The busiest hour is amber and labelled; everything else is ink.
 */
export function HourDots({
  hours,
  peak,
  peakLabel,
  perDot = 1,
}: {
  hours: readonly number[];
  peak: number;
  peakLabel: string;
  perDot?: number;
}) {
  const columns = hours.map((n) => Math.ceil(n / perDot));
  const tallest = Math.max(1, ...columns);
  const step = Math.min(12, DOTS_ROOM / tallest);
  const r = Math.min(5, step * 0.42);
  const centre = (h: number) => DOTS_LEFT + h * DOTS_COL + DOTS_COL / 2;
  const dotY = (k: number) => DOTS_BASE - r - 3 - k * step;
  const axis = [
    [0, "12 am", "start"],
    [6, "6 am", "middle"],
    [12, "12 pm", "middle"],
    [18, "6 pm", "middle"],
    [24, "12 am", "end"],
  ] as const;
  let order = 0;
  return (
    <svg className="nt-hourdots" viewBox="0 0 400 164" aria-hidden>
      <line
        className="nt-hourdots-rule"
        x1={DOTS_LEFT}
        x2={DOTS_LEFT + 24 * DOTS_COL}
        y1={DOTS_BASE + 2}
        y2={DOTS_BASE + 2}
      />
      {Array.from({ length: 25 }, (_, h) => (
        <line
          // biome-ignore lint/suspicious/noArrayIndexKey: the hour is the identity
          key={h}
          className="nt-hourdots-rule"
          x1={DOTS_LEFT + h * DOTS_COL}
          x2={DOTS_LEFT + h * DOTS_COL}
          y1={DOTS_BASE + 2}
          y2={DOTS_BASE + (h % 6 ? 5 : 8)}
        />
      ))}
      {columns.map((n, h) =>
        Array.from({ length: n }, (_, k) => (
          <circle
            // biome-ignore lint/suspicious/noArrayIndexKey: hour and height are the identity
            key={`${h}-${k}`}
            className="nt-hourdots-dot"
            data-peak={h === peak || undefined}
            cx={centre(h)}
            cy={dotY(k)}
            r={r}
            style={{ "--i": order++ } as CSSProperties}
          />
        )),
      )}
      {(columns[peak] ?? 0) > 0 && (
        <text
          className="nt-hourdots-peak"
          x={centre(peak)}
          y={dotY((columns[peak] ?? 1) - 1) - r - 8}
          textAnchor="middle"
        >
          {peakLabel}
        </text>
      )}
      {axis.map(([h, label, anchor]) => (
        <text
          key={h}
          className="nt-hourdots-label"
          x={DOTS_LEFT + h * DOTS_COL}
          y={DOTS_BASE + 22}
          textAnchor={anchor}
        >
          {label}
        </text>
      ))}
    </svg>
  );
}

export interface LineListItem {
  id: string;
  text: string;
  /** Where it came from, under the line. */
  meta: string;
  onOpen?: () => void;
  openLabel?: string;
}

/** A few short lines in the quote serif, each a button to where it came from. */
export function LineList({ items }: { items: LineListItem[] }) {
  return (
    <ul className="nt-linelist">
      {items.map((item, i) => (
        <li key={item.id} style={{ "--i": i } as CSSProperties}>
          <button type="button" onClick={item.onOpen} aria-label={item.openLabel}>
            <span className="nt-linelist-text">{item.text}</span>
            <span className="nt-linelist-meta">{item.meta}</span>
          </button>
        </li>
      ))}
    </ul>
  );
}

// ---------- Pieces for the newer faces ---------- //

export interface GlanceBarRow {
  id: string;
  label: ReactNode;
  value: number;
  /** The number shown at the end of the bar. */
  display: string;
  /** 1..8 for a speaker colour, "accent" for the face's accent, none for quiet ink. */
  color?: number | "accent";
  /** Set the label in the serif, for quoted phrases. */
  serif?: boolean;
}

/** Labelled bars that grow in from the left, scaled to the longest. */
export function GlanceBars({ rows }: { rows: GlanceBarRow[] }) {
  const max = Math.max(1, ...rows.map((r) => r.value));
  return (
    <div className="nt-gbars">
      {rows.map((r, i) => (
        <div key={r.id} className="nt-gbar">
          <span className={cn("nt-gbar-label", r.serif && "nt-gbar-serif")}>{r.label}</span>
          <span className="nt-gbar-track">
            <i
              style={
                {
                  "--f": r.value / max,
                  "--i": i,
                  "--c":
                    r.color === "accent"
                      ? "var(--accent)"
                      : r.color
                        ? `var(--speaker-${r.color})`
                        : "var(--ink-4)",
                } as CSSProperties
              }
            />
          </span>
          <span className="nt-gbar-n">{r.display}</span>
        </div>
      ))}
    </div>
  );
}

/** A meeting's usual length in quiet ink, and the overrun striped in the accent. */
export function OvertimeTrack({
  usualMs,
  actualMs,
  usualLabel,
  actualLabel,
}: {
  usualMs: number;
  actualMs: number;
  usualLabel: string;
  actualLabel: string;
}) {
  const usualPct = Math.min(100, (usualMs / Math.max(usualMs, actualMs)) * 100);
  return (
    <div className="nt-overtime">
      <div className="nt-overtime-track">
        <span className="nt-overtime-usual" style={{ width: `${usualPct}%` }} />
        <span className="nt-overtime-over" style={{ left: `${usualPct}%` }} />
      </div>
      <div className="nt-overtime-ticks">
        <span>0</span>
        <span style={{ left: `${usualPct}%` }}>{usualLabel}</span>
        <span>{actualLabel}</span>
      </div>
    </div>
  );
}

/** A meeting as a line, with a diamond at each decision and a flag on the last one. */
export function DecisionTimeline({
  durationMs,
  marks,
  flag,
  start,
  end,
}: {
  durationMs: number;
  marks: number[];
  flag: string;
  start: string;
  end: string;
}) {
  const pct = (ms: number) => Math.min(100, Math.max(0, (ms / Math.max(1, durationMs)) * 100));
  const last = marks[marks.length - 1];
  return (
    <div className="nt-decisions">
      {last !== undefined && (
        <div className="nt-decisions-flag">
          <span style={{ left: `${pct(last)}%` }}>{flag}</span>
        </div>
      )}
      <div className="nt-decisions-track">
        {[...new Set(marks)].map((ms, i) => (
          <span
            key={ms}
            className="nt-decisions-mark"
            style={{ left: `${pct(ms)}%`, "--i": i } as CSSProperties}
          />
        ))}
      </div>
      <div className="nt-decisions-ticks">
        <span>{start}</span>
        <span>{end}</span>
      </div>
    </div>
  );
}

/** A blinking text caret. Still under Reduce motion. */
export function Caret() {
  return <span className="nt-caret" aria-hidden />;
}

/** A note card with a title and nothing under it but the caret. */
export function BareNoteCard({
  title,
  meta,
  onOpen,
}: {
  title: string;
  meta: string;
  onOpen?: () => void;
}) {
  return (
    <button type="button" className="nt-bare-note" onClick={onOpen} aria-label={`Open ${title}`}>
      <b>{title}</b>
      <Caret />
      <small>{meta}</small>
    </button>
  );
}

/** A word drawn misspelt, with a spellcheck squiggle under it. */
export function Misspelt({ word }: { word: string }) {
  return <span className="nt-misspelt">{word}</span>;
}

/** One bar split in two labelled parts: the quiet one, then the accent one. */
export function SplitBar({
  quiet,
  loud,
}: {
  quiet: { label: string; pct: number };
  loud: { label: string; pct: number };
}) {
  return (
    <div className="nt-splitbar">
      <span className="nt-splitbar-quiet" style={{ flexGrow: quiet.pct }}>
        {quiet.label}
      </span>
      <span className="nt-splitbar-loud" style={{ flexGrow: loud.pct }}>
        {loud.label}
      </span>
    </div>
  );
}

/** An edit Cilica made and you undid: what it said, struck out, and your line kept. */
export function UndoneCard({
  summary,
  struck,
  kept,
}: {
  summary: string;
  struck?: string | null;
  kept?: string | null;
}) {
  return (
    <div className="nt-undone">
      <div className="nt-undone-head">
        <span>{summary}</span>
        <span className="nt-undone-pill">Undone</span>
      </div>
      {struck && <del>{struck}</del>}
      {kept && <span className="nt-undone-kept">{kept}</span>}
    </div>
  );
}
