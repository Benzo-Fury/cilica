/**
 * Cilica's conversation list (shown when no conversation is open): uppercase caption groups and
 * rows with a title, a quiet meta line and a trash button that shows on hover or focus.
 */
import { Trash2 } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "./cn";
import { IconButton } from "./IconButton";

export function ConversationGroup({ label, children }: { label: string; children: ReactNode }) {
  return (
    <section className="nt-conv-group" aria-label={label}>
      <h2 className="nt-conv-caption">{label}</h2>
      {children}
    </section>
  );
}

export interface ConversationRowProps {
  title: string;
  meta: string;
  onOpen: () => void;
  onDelete: () => void;
  /** Title and meta on one line (the floating bar). */
  inline?: boolean;
}

export function ConversationRow({ title, meta, onOpen, onDelete, inline }: ConversationRowProps) {
  return (
    <div className={cn("nt-conv-row nt-anim-host", inline && "nt-conv-row-inline")}>
      <button type="button" className="nt-conv-open" onClick={onOpen}>
        <span className="nt-conv-title">{title}</span>
        <span className="nt-conv-meta">{meta}</span>
      </button>
      <IconButton
        icon={Trash2}
        iconMotion="trash"
        size="sm"
        label={`Delete “${title}”`}
        className="nt-conv-delete"
        onClick={onDelete}
      />
    </div>
  );
}

/** The quiet line under the list, with an optional link. */
export function ConversationFootnote({ children }: { children: ReactNode }) {
  return <p className="nt-conv-footnote">{children}</p>;
}
