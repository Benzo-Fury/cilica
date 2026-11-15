/**
 * Saved people: the list of everyone Cilica can name, and one person's page (their name and
 * colour, how well Cilica knows their voice, and the meetings they were in). No pictures: a
 * person is their name, their colour and their voice.
 */
import { AudioLines, ChevronRight, TriangleAlert } from "lucide-react";
import type { CSSProperties, KeyboardEvent, ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { voiceColor } from "./Voiceprint";

/** How well Cilica knows a voice, as drawn. */
export type VoiceTone = "known" | "learning" | "unknown";

/** A dot and a word for how well Cilica knows someone's voice. */
export function VoiceStatus({ tone, children }: { tone: VoiceTone; children: ReactNode }) {
  return (
    <span className={cn("nt-vstatus", `nt-vstatus-${tone}`)}>
      <i aria-hidden />
      {children}
    </span>
  );
}

/** The list of people. Rows rise in one after another. */
export function PeopleList({ children, label }: { children: ReactNode; label: string }) {
  return (
    <section className="nt-people" aria-label={label}>
      {children}
    </section>
  );
}

/** One person in the list: opens their page. */
export function PersonRow({
  color,
  name,
  meta,
  status,
  strip,
  index = 0,
  onOpen,
  onKeyDown,
}: {
  color: number;
  name: string;
  meta: string;
  status: ReactNode;
  strip?: ReactNode;
  index?: number;
  onOpen: () => void;
  onKeyDown?: (e: KeyboardEvent<HTMLButtonElement>) => void;
}) {
  return (
    <button
      type="button"
      className="nt-people-row nt-anim-host"
      style={{ ...voiceColor(color), "--i": Math.min(index, 12) } as CSSProperties}
      onClick={onOpen}
      onKeyDown={onKeyDown}
    >
      <span className="nt-people-dot" aria-hidden />
      <span className="nt-people-who">
        <b>{name}</b>
        <span>{meta}</span>
      </span>
      {strip && <span className="nt-people-strip">{strip}</span>}
      <span className="nt-people-status">{status}</span>
      <Icon icon={ChevronRight} className="nt-people-chev" />
    </button>
  );
}

/** Shown in place of the list when Find matches no one. */
export function PeopleNone({ children }: { children: ReactNode }) {
  return <p className="nt-people-none">{children}</p>;
}

/** The top of a person's page: their name (editable), a line about them, their colour. */
export function PersonHead({
  color,
  name,
  meta,
  colour,
}: {
  color: number;
  name: ReactNode;
  meta: ReactNode;
  colour?: ReactNode;
}) {
  return (
    <header className="nt-who-head" style={voiceColor(color)}>
      {name}
      <p className="nt-who-meta">{meta}</p>
      {colour && <div className="nt-who-colour">{colour}</div>}
    </header>
  );
}

/**
 * A person's name as the page heading, renamed in place: Enter or leaving the field saves,
 * Esc puts the old name back. An empty name is not saved.
 */
export function NameField({
  value,
  label,
  onCommit,
  focusKey,
}: {
  value: string;
  label: string;
  onCommit: (name: string) => void;
  /** Change it to put the caret in the field (Rename in a menu). */
  focusKey?: number;
}) {
  const [draft, setDraft] = useState(value);
  const ref = useRef<HTMLInputElement>(null);
  const [synced, setSynced] = useState(value);
  if (value !== synced) {
    setSynced(value);
    setDraft(value);
  }
  useEffect(() => {
    if (!focusKey) return;
    ref.current?.focus();
    ref.current?.select();
  }, [focusKey]);
  const commit = () => {
    const name = draft.trim();
    if (name && name !== value) onCommit(name);
    else setDraft(value);
  };
  return (
    <input
      ref={ref}
      className="nt-who-name"
      value={draft}
      aria-label={label}
      spellCheck={false}
      autoComplete="off"
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          e.currentTarget.blur();
        } else if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          setDraft(value);
          requestAnimationFrame(() => ref.current?.blur());
        }
      }}
    />
  );
}

/** A line under the heading suggesting this may be someone saved twice. */
export function DuplicateNotice({ children, action }: { children: ReactNode; action: ReactNode }) {
  return (
    <div className="nt-who-dup" role="note">
      <span>{children}</span>
      {action}
    </div>
  );
}

/**
 * How well Cilica knows the voice, as an open section like the meetings under it: the status
 * beside the heading, a sentence on why, then the voiceprint drawn straight on the page.
 */
export function VoiceSection({
  tone,
  status,
  headline,
  body,
  children,
  action,
}: {
  tone: VoiceTone;
  status: string;
  headline: string;
  body: string;
  /** The voiceprint and its figures. */
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <section className={cn("nt-vsection", `nt-vsection-${tone}`)} aria-label="Voice">
      <SectionHead title="Voice" end={<VoiceStatus tone={tone}>{status}</VoiceStatus>} />
      <p className="nt-vsection-text">
        <b>{headline}.</b> {body}
      </p>
      {children}
      {action && <div className="nt-vsection-action">{action}</div>}
    </section>
  );
}

/** Figures under a voiceprint, on one line. */
export function VoiceStats({ children }: { children: ReactNode }) {
  return <div className="nt-vstats">{children}</div>;
}

export function VoiceStat({
  value,
  label,
  meter,
}: {
  value: ReactNode;
  label: string;
  /** 0..1: a short bar in the person's colour. */
  meter?: number;
}) {
  return (
    <span className="nt-vstat">
      <b>{value}</b>
      <span>{label}</span>
      {meter !== undefined && (
        <span className="nt-vstat-meter" aria-hidden>
          <i style={{ "--p": Math.max(0, Math.min(1, meter)) } as CSSProperties} />
        </span>
      )}
    </span>
  );
}

/** A section heading on a person's page, with a count and room for a control. */
export function SectionHead({
  title,
  count,
  end,
}: {
  title: string;
  count?: number;
  end?: ReactNode;
}) {
  return (
    <div className="nt-who-section">
      <h2>
        {title}
        {count !== undefined && <span className="nt-who-section-count">{count}</span>}
      </h2>
      {end}
    </div>
  );
}

export function MeetingList({ children }: { children: ReactNode }) {
  return <div className="nt-mlist">{children}</div>;
}

/**
 * One meeting a person was in: when, what, how much of it they spoke, and whether Cilica
 * learnt their voice from it. The row opens the transcript; `actions` sit on the right.
 */
export function MeetingRow({
  index = 0,
  day,
  time,
  title,
  meta,
  share,
  learnt,
  odd,
  tag,
  actions,
  onOpen,
  openLabel,
}: {
  index?: number;
  day: string;
  time: string;
  title: string;
  meta: string;
  /** 0..1 of the meeting they spoke, when known. */
  share: number | null;
  /** Cilica learnt their voice from this meeting: the label for the mark. */
  learnt?: string;
  /** That meeting sounds unlike the others: the label for the warning. */
  odd?: string;
  /** A quiet word on the right ("Named by Cilica"). */
  tag?: string;
  actions?: ReactNode;
  onOpen: () => void;
  openLabel: string;
}) {
  return (
    <div
      className={cn("nt-mrow", odd && "nt-mrow-odd")}
      style={{ "--i": Math.min(index, 10) } as CSSProperties}
    >
      <button type="button" className="nt-mrow-open" onClick={onOpen} aria-label={openLabel}>
        <span className="nt-mrow-when">
          <b>{day}</b>
          <span>{time}</span>
        </span>
        <span className="nt-mrow-what">
          <b title={title}>{title}</b>
          <span>{meta}</span>
        </span>
        <span
          className="nt-mrow-share"
          title={share === null ? undefined : `Spoke ${Math.round(share * 100)}% of the meeting`}
          aria-hidden
        >
          {share !== null && <i style={{ "--p": share.toFixed(3) } as CSSProperties} />}
        </span>
        <span className="nt-mrow-marks">
          {odd ? (
            <span className="nt-mrow-mark nt-mrow-warn" title={odd}>
              <Icon icon={TriangleAlert} />
              <span className="nt-sr-only">{odd}</span>
            </span>
          ) : learnt ? (
            <span className="nt-mrow-mark" title={learnt}>
              <Icon icon={AudioLines} />
              <span className="nt-sr-only">{learnt}</span>
            </span>
          ) : null}
          {tag && <span className="nt-mrow-tag">{tag}</span>}
        </span>
      </button>
      {actions && <span className="nt-mrow-ops">{actions}</span>}
    </div>
  );
}

/** The quiet button under a list ("Show all 23"). */
export function ListMore({ children, onClick }: { children: ReactNode; onClick: () => void }) {
  return (
    <button type="button" className="nt-list-more nt-anim" onClick={onClick}>
      {children}
    </button>
  );
}

/** Before a person has been in a meeting. */
export function MeetingsEmpty({ children }: { children: ReactNode }) {
  return <p className="nt-mlist-empty">{children}</p>;
}

/** The foot of a person's page: what managing them keeps, and the buttons. */
export function PersonFoot({ note, children }: { note: string; children: ReactNode }) {
  return (
    <footer className="nt-who-foot">
      <span className="nt-who-foot-note">{note}</span>
      <div>{children}</div>
    </footer>
  );
}
