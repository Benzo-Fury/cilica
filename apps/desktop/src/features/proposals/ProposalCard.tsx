/**
 * One card per proposal kind (DiffCard doc, spec › Guardrails). Edits show a prose diff with
 * per-hunk accept; renames, merges, moves, deletions and new notes are confirm cards that name
 * every note they touch. Accepting lifts the card away (`smooth`) and, for an edit, washes the
 * changed sentences in the note. A `conflict` from Apply is explained on the card, which stays
 * pending.
 */
import { ArrowRight, Check, FileText, TriangleAlert, X } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { type ReactNode, useMemo, useState } from "react";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { openNote, useOpenNoteId } from "@/app/router";
import { useAiGlow } from "@/features/note/aiGlow";
import type { NoteRef, Proposal } from "@/ipc/types";
import { displayTitle, plural } from "@/lib/format";
import { blockProse } from "@/lib/markdown";
import { finalMarkdown, proseDiff } from "@/lib/proseDiff";
import { Button, Checkbox, DiffCard, Icon, Markdown, ProseDiffRows, Skeleton } from "@/ui";
import { isConflict, useApplyProposal, useDismissProposal, useProposal } from "./hooks";
import { kindIcon } from "./tools";

/** Chat cards answer Accept / Reject; task reports Apply / Dismiss (spec › Scheduled tasks). */
export type CardVerbs = "chat" | "task";

export function ProposalCard({
  proposalId,
  initial,
  undoToast = true,
  verbs = "chat",
}: {
  proposalId: string;
  initial?: Proposal;
  undoToast?: boolean;
  verbs?: CardVerbs;
}) {
  const { data: proposal } = useProposal(proposalId, initial);
  if (!proposal) return <Skeleton className="nt-proposal-skel" />;
  return <ProposalView proposal={proposal} undoToast={undoToast} verbs={verbs} />;
}

export function ProposalView({
  proposal,
  undoToast = true,
  verbs = "chat",
}: {
  proposal: Proposal;
  undoToast?: boolean;
  verbs?: CardVerbs;
}) {
  const reduced = useReducedMotion();
  const pending = proposal.status === "pending";
  return (
    <AnimatePresence mode="popLayout" initial={false}>
      {pending ? (
        <motion.div
          key="pending"
          layout="position"
          exit={{
            opacity: 0,
            y: reduced ? 0 : -4,
            scale: reduced ? 1 : 0.98,
            transition: reduced ? reducedTransition : { ...springs.smooth, opacity: tweens.base },
          }}
        >
          <PendingCard proposal={proposal} undoToast={undoToast} verbs={verbs} />
        </motion.div>
      ) : (
        <motion.div
          key="resolved"
          layout="position"
          initial={{ opacity: 0, y: reduced ? 0 : 4 }}
          animate={{ opacity: 1, y: 0, transition: reduced ? reducedTransition : springs.smooth }}
        >
          <ResolvedRow proposal={proposal} />
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function headline(proposal: Proposal): { action: string; subject: string } {
  const k = proposal.kind;
  switch (k.type) {
    case "edit_note":
      return { action: "Edit", subject: displayTitle(k.note.title) };
    case "rename_note":
      return { action: "Rename", subject: displayTitle(k.note.title) };
    case "trash_notes":
      return { action: "Move to Trash", subject: plural(k.notes.length, "note") };
    case "merge_notes":
      return { action: "Merge", subject: plural(k.sources.length, "note") };
    case "delete_category":
      return { action: "Delete category", subject: k.name };
    case "delete_person":
      return { action: "Delete person", subject: k.name };
    case "bulk_move":
      return { action: "Move", subject: `${plural(k.notes.length, "note")} to ${k.category_name}` };
    case "create_note":
      return { action: "New note", subject: displayTitle(k.title) };
  }
}

function ResolvedRow({ proposal }: { proposal: Proposal }) {
  const { action, subject } = headline(proposal);
  const applied = proposal.status === "applied";
  return (
    <div className="nt-proposal-done">
      <Icon icon={applied ? Check : X} size="sm" />
      <span>
        {applied ? "Applied" : "Dismissed"} · {action} {subject}
      </span>
    </div>
  );
}

function NoteList({ notes }: { notes: readonly NoteRef[] }) {
  return (
    <ul className="nt-proposal-notes">
      {notes.map((n) => (
        <li key={n.id}>
          <button type="button" className="nt-proposal-note" onClick={() => openNote(n.id)}>
            <Icon icon={FileText} size="sm" />
            <span>{displayTitle(n.title)}</span>
          </button>
        </li>
      ))}
    </ul>
  );
}

function Preview({ title, markdown }: { title: string; markdown: string }) {
  return (
    <div className="nt-proposal-preview nt-selectable">
      <div className="nt-proposal-preview-title">{displayTitle(title)}</div>
      <Markdown source={markdown.replace(/^#\s+.*\n+/, "")} />
    </div>
  );
}

function PendingCard({
  proposal,
  undoToast,
  verbs,
}: {
  proposal: Proposal;
  undoToast: boolean;
  verbs: CardVerbs;
}) {
  const apply = useApplyProposal({ undoToast });
  const dismiss = useDismissProposal();
  const openNoteId = useOpenNoteId();
  const [conflict, setConflict] = useState(false);
  const kind = proposal.kind;
  const diff = useMemo(
    () => (kind.type === "edit_note" ? proseDiff(kind.before_md, kind.after_md) : null),
    [kind],
  );
  const hunkCount = diff?.hunks.length ?? 0;
  const [accepted, setAccepted] = useState<ReadonlySet<number>>(
    () => new Set(Array.from({ length: hunkCount }, (_, i) => i)),
  );
  const { action, subject } = headline(proposal);
  const busy = apply.isPending || dismiss.isPending;
  const destructive = proposal.tier === "destructive";

  const onApply = () => {
    let finalMd: string | null = null;
    if (diff && accepted.size < hunkCount) finalMd = finalMarkdown(diff, accepted);
    apply.mutate(
      { proposalId: proposal.id, finalMd },
      {
        onSuccess: () => {
          if (diff && kind.type === "edit_note" && kind.note.id === openNoteId) {
            const phrases = diff.hunks
              .filter((h) => accepted.has(h.index))
              .flatMap((h) => h.after.map(blockProse));
            useAiGlow.getState().request(kind.note.id, phrases);
          }
        },
        onError: (error) => setConflict(isConflict(error)),
      },
    );
  };

  let body: ReactNode;
  let acceptLabel: string;
  let rejectLabel = "Reject";
  switch (kind.type) {
    case "edit_note": {
      const verb = verbs === "task" ? "Apply" : "Accept";
      acceptLabel =
        hunkCount > 1 && accepted.size < hunkCount
          ? `${verb} ${accepted.size} of ${hunkCount}`
          : verb;
      body = (
        <div className="nt-hunks">
          {diff?.hunks.map((hunk) => {
            const on = accepted.has(hunk.index);
            return (
              <div key={hunk.index} className="nt-hunk" data-off={on ? undefined : ""}>
                {hunkCount > 1 && (
                  <Checkbox
                    checked={on}
                    label={`Keep change ${hunk.index + 1}${hunk.section ? ` in ${hunk.section}` : ""}`}
                    onCheckedChange={(checked) =>
                      setAccepted((set) => {
                        const next = new Set(set);
                        if (checked) next.add(hunk.index);
                        else next.delete(hunk.index);
                        return next;
                      })
                    }
                  />
                )}
                <div className="nt-hunk-body">
                  {hunk.section && hunkCount > 1 && hunk.rows[0]?.kind !== "heading" && (
                    <div className="nt-hunk-section">{hunk.section}</div>
                  )}
                  <ProseDiffRows rows={hunk.rows} />
                </div>
              </div>
            );
          })}
          {hunkCount === 0 && <p className="nt-faint">This edit changes nothing you can see.</p>}
        </div>
      );
      break;
    }
    case "rename_note":
      acceptLabel = "Rename";
      rejectLabel = "Keep title";
      body = (
        <p className="nt-proposal-rename">
          <del>{displayTitle(kind.note.title)}</del>
          <Icon icon={ArrowRight} size="sm" className="nt-faint" />
          <ins>{kind.new_title}</ins>
        </p>
      );
      break;
    case "trash_notes":
      acceptLabel =
        kind.notes.length === 1 ? "Move to Trash" : `Move ${kind.notes.length} notes to Trash`;
      rejectLabel = "Keep";
      body = (
        <>
          <NoteList notes={kind.notes} />
          <p className="nt-proposal-note-line">Notes stay in Trash for 30 days.</p>
        </>
      );
      break;
    case "merge_notes":
      acceptLabel = `Merge ${kind.sources.length} notes`;
      rejectLabel = "Keep separate";
      body = (
        <>
          <NoteList notes={kind.sources} />
          <p className="nt-proposal-note-line">
            They become one note and the originals move to Trash:
          </p>
          <Preview title={kind.merged_title} markdown={kind.merged_md} />
        </>
      );
      break;
    case "delete_category":
      acceptLabel = `Delete ${kind.name}`;
      rejectLabel = "Keep";
      body = (
        <p className="nt-proposal-note-line">
          {kind.note_count === 0
            ? "The category is empty."
            : `Its ${plural(kind.note_count, "note")} move to Notes. No note is deleted.`}
        </p>
      );
      break;
    case "delete_person":
      acceptLabel = `Delete ${kind.name}`;
      rejectLabel = "Keep";
      body = (
        <p className="nt-proposal-note-line">
          Cilica forgets their voice. Past meetings keep the speaker's name.
        </p>
      );
      break;
    case "bulk_move":
      acceptLabel = `Move ${plural(kind.notes.length, "note")}`;
      rejectLabel = "Leave them";
      body = (
        <>
          <NoteList notes={kind.notes} />
          <p className="nt-proposal-note-line">To {kind.category_name}.</p>
        </>
      );
      break;
    case "create_note":
      acceptLabel = "Create note";
      rejectLabel = "Dismiss";
      body = (
        <>
          <p className="nt-proposal-note-line">In {kind.category_name ?? "Notes"}:</p>
          <Preview title={kind.title} markdown={kind.body_md} />
        </>
      );
      break;
  }

  if (verbs === "task") rejectLabel = "Dismiss";
  const single = diff?.hunks.length === 1 ? diff.hunks[0]?.section : null;
  return (
    <DiffCard
      className="nt-proposal"
      head={
        <>
          <Icon icon={kindIcon(kind.type)} size="sm" />
          <strong>{action}</strong>
          <span className="nt-proposal-subject">
            {subject}
            {single ? ` · ${single}` : ""}
          </span>
          {hunkCount > 1 && (
            <span className="nt-proposal-count">{plural(hunkCount, "change")}</span>
          )}
        </>
      }
      foot={
        <>
          <span className="nt-faint nt-diff-foot-note">
            {destructive
              ? "Asks every time"
              : kind.type === "edit_note"
                ? "Needs your review"
                : "Waiting for you"}
          </span>
          <Button
            size="sm"
            variant="ghost"
            disabled={busy}
            loading={dismiss.isPending}
            onClick={() => dismiss.mutate(proposal.id)}
          >
            {rejectLabel}
          </Button>
          <Button
            size="sm"
            variant={destructive ? "danger" : "primary"}
            className={destructive ? "nt-btn-danger-solid" : undefined}
            icon={destructive ? undefined : Check}
            iconMotion="check"
            disabled={busy || (diff !== null && accepted.size === 0)}
            loading={apply.isPending}
            onClick={onApply}
          >
            {acceptLabel}
          </Button>
        </>
      }
    >
      {proposal.reason && <p className="nt-proposal-reason">{proposal.reason}</p>}
      {conflict && (
        <p className="nt-proposal-warn" role="alert">
          <Icon icon={TriangleAlert} size="sm" />
          <span>
            {kind.type === "edit_note"
              ? "This note changed after Cilica suggested this edit. Ask Cilica again for a fresh suggestion, or reject this one."
              : "Something changed after Cilica suggested this. Ask Cilica again, or dismiss this one."}
          </span>
        </p>
      )}
      {body}
    </DiffCard>
  );
}
