/**
 * Assistant thread pieces (AssistantThread doc): your turns as `fill-press` bubbles, Cilica's as
 * plain text, tool steps as single `ink-3` lines that expand, and the shimmering working label.
 */
import type { LucideIcon } from "lucide-react";
import { ChevronRight } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";
import { type ReactNode, useId, useState } from "react";
import { reducedTransition, springs } from "@/app/motion";
import { Collapse } from "./Collapse";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { Shimmer } from "./Skeleton";

/** Messages enter with a short rise (`snappy`); they never bounce. */
export function ThreadItem({ children, className }: { children: ReactNode; className?: string }) {
  const reduced = useReducedMotion();
  return (
    <motion.div
      className={className}
      initial={{ opacity: 0, y: reduced ? 0 : 6 }}
      animate={{ opacity: 1, y: 0, transition: reduced ? reducedTransition : springs.snappy }}
    >
      {children}
    </motion.div>
  );
}

export function Bubble({ children }: { children: ReactNode }) {
  return <div className="nt-bubble nt-selectable">{children}</div>;
}

export function AiText({ children }: { children: ReactNode }) {
  return <div className="nt-ai nt-ai-app nt-selectable">{children}</div>;
}

/** A one-line tool step ("Searched 14 notes") that opens to show its details. */
export function ToolStep({
  icon,
  label,
  working = false,
  failed = false,
  details,
}: {
  icon: LucideIcon;
  label: string;
  working?: boolean;
  failed?: boolean;
  details?: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  if (working) {
    return (
      <div className="nt-step">
        <Icon icon={icon} size="sm" />
        <Shimmer>{label}</Shimmer>
      </div>
    );
  }
  return (
    <div className={cn("nt-step-wrap", failed && "nt-step-failed")}>
      <div className="nt-step">
        <button
          type="button"
          aria-expanded={details ? open : undefined}
          aria-controls={details ? id : undefined}
          disabled={!details}
          onClick={() => setOpen((o) => !o)}
        >
          <Icon icon={icon} size="sm" />
          <span>{label}</span>
          {details && (
            <ChevronRight className="nt-ico nt-ico-sm nt-chevron" strokeWidth={1.75} aria-hidden />
          )}
        </button>
      </div>
      {details && (
        <Collapse open={open} id={id}>
          <div className="nt-step-details">{details}</div>
        </Collapse>
      )}
    </div>
  );
}

/** An error line in the thread: what went wrong and, optionally, an action that fixes it. */
export function ThreadNotice({
  icon,
  children,
  tone = "neutral",
  action,
}: {
  icon: LucideIcon;
  children: ReactNode;
  tone?: "neutral" | "danger";
  /** Shown on its own line under the message, so it never wraps into the sentence. */
  action?: ReactNode;
}) {
  return (
    <div
      role={tone === "danger" ? "alert" : undefined}
      className={cn("nt-thread-notice", tone === "danger" && "nt-thread-notice-danger")}
    >
      <Icon icon={icon} size="sm" />
      <div className="nt-thread-notice-body">
        <p>{children}</p>
        {action && <div className="nt-thread-notice-action">{action}</div>}
      </div>
    </div>
  );
}

/** Cilica's suggestions at rest: one card, a row each, divided by hairlines. */
export function SuggestionList({
  label,
  busy,
  children,
}: {
  label: string;
  busy?: boolean;
  children: ReactNode;
}) {
  return (
    <section className="nt-suggest" aria-label={label} aria-busy={busy || undefined}>
      {children}
    </section>
  );
}

/** One suggestion: the glyph sits on the row itself, never in a tile. */
export function SuggestionRow({
  icon,
  title,
  detail,
  onClick,
}: {
  icon: LucideIcon;
  title: string;
  detail?: string;
  onClick: () => void;
}) {
  return (
    <button type="button" className="nt-suggest-row nt-anim-host" onClick={onClick}>
      <Icon icon={icon} size="sm" className="nt-suggest-icon" />
      <span className="nt-suggest-text">
        <span className="nt-suggest-title">{title}</span>
        {detail && <span className="nt-suggest-detail">{detail}</span>}
      </span>
    </button>
  );
}

/** Placeholder rows while suggestions load. */
export function SuggestionSkeleton({ rows = 3 }: { rows?: number }) {
  return (
    <>
      {Array.from({ length: rows }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: static placeholder rows
        <span key={i} className="nt-suggest-row nt-suggest-skel" aria-hidden>
          <span className="nt-skel nt-suggest-skel-icon" />
          <span className="nt-suggest-text">
            <span className="nt-skel nt-suggest-skel-line" />
            <span className="nt-skel nt-suggest-skel-line nt-skel-short" />
          </span>
        </span>
      ))}
    </>
  );
}

export interface TouchItem {
  key: string;
  label: string;
  verb: string;
  live: boolean;
}

/** What a running turn is touching: one pill per note, the live one outlined. */
export function TouchList({ items }: { items: TouchItem[] }) {
  if (items.length === 0) return null;
  return (
    <div className="nt-touch">
      <span className="nt-touch-caption">Touching</span>
      <ul className="nt-touch-list">
        {items.map((item) => (
          <li key={item.key} className="nt-touch-item" data-live={item.live || undefined}>
            <span className="nt-touch-label">{item.label}</span>
            <span className="nt-touch-verb">{item.verb}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export interface ChangeRow {
  key: string;
  icon: LucideIcon;
  label: string;
  quiet?: boolean;
  onOpen?: () => void;
}

/** "What changed" under a finished turn: one row per change, then the footer (Undo all). */
export function ChangeCard({
  title,
  icon,
  rows,
  footer,
}: {
  title: string;
  icon: LucideIcon;
  rows: ChangeRow[];
  footer?: ReactNode;
}) {
  return (
    <section className="nt-changes" aria-label={title}>
      <header className="nt-changes-head">
        <Icon icon={icon} size="sm" className="nt-changes-icon" />
        <span>{title}</span>
      </header>
      <ul className="nt-changes-list">
        {rows.map((row) => (
          <li key={row.key} className="nt-changes-row" data-quiet={row.quiet || undefined}>
            <Icon icon={row.icon} size="sm" />
            {row.onOpen ? (
              <button type="button" className="nt-changes-open" onClick={row.onOpen}>
                {row.label}
              </button>
            ) : (
              <span>{row.label}</span>
            )}
          </li>
        ))}
      </ul>
      {footer && <footer className="nt-changes-foot">{footer}</footer>}
    </section>
  );
}
