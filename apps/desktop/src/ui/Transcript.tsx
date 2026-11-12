/**
 * Transcript pieces (Transcript doc): one speaker turn per row with a 24px avatar, the name in
 * 13px semibold, a mono timecode, and the text in `transcript` 15/23. Colour lives only on the
 * avatar ring. Live words arrive grey with a 2px blur clearing, then settle to ink.
 */
import { ArrowDown } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";
import type { ButtonHTMLAttributes, ReactNode, Ref } from "react";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { useSpecular } from "./useSpecular";

export function Utterance({
  avatar,
  name,
  time,
  extra,
  children,
  current,
  hit,
  className,
  ref,
}: {
  avatar: ReactNode;
  name: ReactNode;
  time: ReactNode;
  /** After the timecode: a "Name this speaker" chip or a suggestion. */
  extra?: ReactNode;
  children: ReactNode;
  /** Playback is inside this turn. */
  current?: boolean;
  /** A search hit landed here; washes briefly. */
  hit?: boolean;
  className?: string;
  ref?: Ref<HTMLDivElement>;
}) {
  return (
    <div
      ref={ref}
      className={cn(
        "nt-utt nt-utt-app",
        current && "nt-utt-current",
        hit && "nt-utt-hit",
        className,
      )}
    >
      {avatar}
      <div className="nt-utt-head">
        {name}
        {time}
        {extra}
      </div>
      <div className="nt-utt-text nt-selectable">{children}</div>
    </div>
  );
}

/** The speaker name as a button that opens naming and linking. */
export function SpeakerButton({ className, ...props }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button type="button" className={cn("nt-utt-name nt-speaker-btn", className)} {...props} />
  );
}

/** A timecode that seeks when the recording can play. */
export function TimeButton({
  children,
  onSeek,
  label,
}: {
  children: ReactNode;
  onSeek?: () => void;
  label: string;
}) {
  if (!onSeek) return <span className="nt-utt-time">{children}</span>;
  return (
    <button type="button" className="nt-utt-time nt-time-btn" aria-label={label} onClick={onSeek}>
      {children}
    </button>
  );
}

/** One segment's text. Partial text is `ink-3`; `fresh` words fade in. */
export function SegmentText({
  text,
  partial,
  live,
  playing,
  onClick,
  marks,
  swears,
  segmentId,
}: {
  text: string;
  partial: boolean;
  /** Animate words as they arrive (live recording). */
  live: boolean;
  playing?: boolean;
  onClick?: () => void;
  /** Character ranges to highlight (find in transcript). `current` is the active match. */
  marks?: readonly { start: number; end: number; current: boolean }[];
  /** Swears that put a coin in the jar this session, washed in amber. Find matches win. */
  swears?: readonly { start: number; end: number }[];
  /** Lets a coin find the word it leaves from. */
  segmentId?: string;
}) {
  const body =
    marks && marks.length > 0
      ? markText(text, marks)
      : swears && swears.length > 0
        ? swearText(text, swears)
        : live
          ? liveWords(text)
          : text;
  const className = cn(
    "nt-seg-text",
    partial && "nt-word-partial",
    playing && "nt-seg-playing",
    onClick && "nt-seg-seek",
  );
  if (onClick) {
    return (
      // biome-ignore lint/a11y/useKeyWithClickEvents lint/a11y/noStaticElementInteractions: the timecode button is the keyboard path to seek
      <span className={className} onClick={onClick} data-segment-id={segmentId}>
        {body}{" "}
      </span>
    );
  }
  return (
    <span className={className} data-segment-id={segmentId}>
      {body}{" "}
    </span>
  );
}

function swearText(text: string, swears: readonly { start: number; end: number }[]): ReactNode {
  const out: ReactNode[] = [];
  let last = 0;
  for (const s of swears) {
    if (s.start > last) out.push(text.slice(last, s.start));
    out.push(
      <span key={s.start} className="nt-swear">
        {text.slice(s.start, s.end)}
      </span>,
    );
    last = s.end;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

function liveWords(text: string): ReactNode {
  const tokens = text.match(/\S+\s*|\s+/g) ?? [];
  return tokens.map((t, i) => (
    // biome-ignore lint/suspicious/noArrayIndexKey: words only append while a segment is partial
    <span key={i} className="nt-word-new">
      {t}
    </span>
  ));
}

function markText(
  text: string,
  marks: readonly { start: number; end: number; current: boolean }[],
): ReactNode {
  const out: ReactNode[] = [];
  let last = 0;
  marks.forEach((m, i) => {
    if (m.start > last) out.push(text.slice(last, m.start));
    out.push(
      <mark
        // biome-ignore lint/suspicious/noArrayIndexKey: matches are positional
        key={i}
        className={cn("nt-find-match", m.current && "nt-find-current")}
        data-find-current={m.current || undefined}
      >
        {text.slice(m.start, m.end)}
      </mark>,
    );
    last = m.end;
  });
  if (last < text.length) out.push(text.slice(last));
  return out;
}

/** The glass "Jump to live" chip that appears when auto-scroll is paused. */
export function JumpChip({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  const reduced = useReducedMotion();
  const specular = useSpecular<HTMLButtonElement>();
  return (
    <motion.button
      type="button"
      className="nt-jump nt-btn nt-btn-glass nt-btn-sm nt-anim"
      onClick={onClick}
      onPointerMove={specular}
      initial={{ opacity: 0, y: reduced ? 0 : 8, scale: reduced ? 1 : 0.96 }}
      animate={{
        opacity: 1,
        y: 0,
        scale: 1,
        transition: reduced ? reducedTransition : springs.bouncy,
      }}
      exit={{ opacity: 0, y: reduced ? 0 : 4, transition: tweens.exit }}
    >
      <Icon icon={ArrowDown} size="sm" motion="arrow" />
      {children}
    </motion.button>
  );
}

/** A quiet inline question ("Is Person 2 Sam?") with two small answers. */
export function InlinePrompt({ children, actions }: { children: ReactNode; actions: ReactNode }) {
  const reduced = useReducedMotion();
  return (
    <motion.span
      className="nt-inline-prompt"
      initial={{ opacity: 0, x: reduced ? 0 : -4 }}
      animate={{ opacity: 1, x: 0, transition: reduced ? reducedTransition : springs.snappy }}
      exit={{ opacity: 0, transition: tweens.exit }}
    >
      <span>{children}</span>
      {actions}
    </motion.span>
  );
}
