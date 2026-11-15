/**
 * Transcript pieces (Transcript doc): one speaker turn per row with a 24px avatar, the name in
 * 13px semibold, a mono timecode, and the text in `transcript` 15/23. Colour lives only on the
 * avatar ring. Live words rise into place grey with a 2px blur clearing, then settle to ink. A
 * line Cilica moves from "Pending" to its speaker glides there word by word (`useTranscriptFlip`).
 */
import { ArrowDown, X } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";
import type { ButtonHTMLAttributes, ReactNode, Ref } from "react";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { IconButton } from "./IconButton";
import { useSpecular } from "./useSpecular";

export function Utterance({
  avatar,
  motion,
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
  /** A new row's avatar and name rise in. */
  motion?: "rise";
  ref?: Ref<HTMLDivElement>;
}) {
  return (
    <div
      ref={ref}
      className={cn(
        "nt-utt nt-utt-app",
        current && "nt-utt-current",
        hit && "nt-utt-hit",
        motion === "rise" && "nt-utt-rise",
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

/** When a turn starts, as plain text. Playing the recording is done from the playback bar. */
export function TurnTime({ children }: { children: ReactNode }) {
  return <span className="nt-utt-time">{children}</span>;
}

/** A line arriving under its speaker from "Pending": its words glide there, no rise. */
export type LineMotion = "arrive";

/** One segment's text. Partial text is `ink-3`; live words rise in as they are heard. */
export function SegmentText({
  text,
  partial,
  live,
  marks,
  swears,
  segmentId,
  motion,
}: {
  text: string;
  partial: boolean;
  /** Animate words as they arrive (live recording). */
  live: boolean;
  /** Character ranges to highlight (find in transcript). `current` is the active match. */
  marks?: readonly { start: number; end: number; current: boolean }[];
  /** Swears that put a coin in the jar this session, washed in amber. Find matches win. */
  swears?: readonly { start: number; end: number }[];
  /** Lets a coin find the word it leaves from. */
  segmentId?: string;
  motion?: LineMotion;
}) {
  const body =
    marks && marks.length > 0
      ? markText(text, marks)
      : live || motion
        ? liveWords(text, swears)
        : swears && swears.length > 0
          ? swearText(text, swears)
          : text;
  const className = cn(
    "nt-seg-text",
    partial && "nt-word-partial",
    motion === "arrive" && "nt-seg-arrive",
  );
  return (
    <span className={className} data-segment-id={segmentId}>
      {body}{" "}
    </span>
  );
}

type Range = { start: number; end: number };

/** `text` (which starts at `offset` in the segment) with its swears washed. */
function swearText(text: string, swears: readonly Range[], offset = 0): ReactNode {
  const out: ReactNode[] = [];
  let last = 0;
  for (const s of swears) {
    const start = Math.max(s.start - offset, last);
    const end = Math.min(s.end - offset, text.length);
    if (end <= start) continue;
    if (start > last) out.push(text.slice(last, start));
    out.push(
      <span key={start} className="nt-swear">
        {text.slice(start, end)}
      </span>,
    );
    last = end;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

/** Word by word, so each can rise in on its own. Swears inside a word keep their wash. */
function liveWords(text: string, swears?: readonly Range[]): ReactNode {
  const tokens = text.match(/\S+\s*|\s+/g) ?? [];
  let offset = 0;
  return tokens.map((t, i) => {
    const at = offset;
    offset += t.length;
    return (
      // biome-ignore lint/suspicious/noArrayIndexKey: words only append while a segment is partial
      <span key={i} className="nt-tword">
        {swears?.length ? swearText(t, swears, at) : t}
      </span>
    );
  });
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

/**
 * A quiet grey notice at the top of a transcript (why its speakers are not labelled, say), with
 * an optional action and a dismiss button. It rises in on `smooth` and fades out quickly.
 */
export function TranscriptNotice({
  icon,
  title,
  children,
  action,
  onDismiss,
}: {
  icon: Parameters<typeof Icon>[0]["icon"];
  title: string;
  children: ReactNode;
  action?: ReactNode;
  onDismiss: () => void;
}) {
  const reduced = useReducedMotion();
  return (
    <motion.div
      className="nt-transcript-notice"
      role="note"
      initial={{ opacity: 0, y: reduced ? 0 : -4 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, transition: tweens.exit }}
      transition={reduced ? reducedTransition : springs.smooth}
    >
      <Icon icon={icon} size="sm" className="nt-transcript-notice-icon" />
      <div className="nt-transcript-notice-body">
        <span className="nt-transcript-notice-title">{title}</span>
        <span className="nt-transcript-notice-text">{children}</span>
        {action && <div className="nt-transcript-notice-action">{action}</div>}
      </div>
      <IconButton icon={X} iconMotion="x" size="sm" label="Dismiss" onClick={onDismiss} />
    </motion.div>
  );
}
