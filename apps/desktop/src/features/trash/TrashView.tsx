/** Trash: notes stay 30 days. Restore, delete forever, or empty it (with a confirm dialog). */

import { Mic, RotateCcw, Trash2 } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useState } from "react";
import { ShortcutTip } from "@/app/keybinds";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { useDeleteForever, useEmptyTrash, useRestoreNote, useTrash } from "@/features/notes/hooks";
import { CanvasBand } from "@/features/shell/CanvasBand";
import type { NoteSummary } from "@/ipc/types";
import { displayTitle, relativeDate } from "@/lib/format";
import {
  Button,
  ConfirmDialog,
  Crumb,
  EmptyState,
  Icon,
  IconButton,
  ListItem,
  PageColumn,
  ScrollArea,
  SkeletonText,
  ViewLayer,
} from "@/ui";

const DAY = 86_400_000;

/** "just now", "yesterday", "Fri", "2 Nov": lower-case only the words that are not names. */
function deletedWhen(iso: string): string {
  const when = relativeDate(iso);
  if (when === "Now") return "just now";
  if (when === "Yesterday") return "yesterday";
  if (/^\d+[mh]$/.test(when)) return `${when} ago`;
  return when;
}

function daysLeft(note: NoteSummary): number {
  if (!note.deleted_at) return 30;
  const left = 30 - Math.floor((Date.now() - new Date(note.deleted_at).getTime()) / DAY);
  return Math.max(0, left);
}

export function TrashView() {
  const reduced = useReducedMotion();
  const { data: notes = [], isLoading } = useTrash();
  const restore = useRestoreNote();
  const remove = useDeleteForever();
  const empty = useEmptyTrash();
  const [confirm, setConfirm] = useState<
    { kind: "empty" } | { kind: "one"; note: NoteSummary } | null
  >(null);

  return (
    <ViewLayer>
      <CanvasBand
        start={<Crumb parts={["Trash"]} />}
        end={
          notes.length > 0 ? (
            <Button
              variant="danger"
              size="sm"
              icon={Trash2}
              iconMotion="trash"
              onClick={() => setConfirm({ kind: "empty" })}
            >
              Empty Trash
            </Button>
          ) : undefined
        }
      />
      <ScrollArea>
        <PageColumn>
          <header className="nt-page-head">
            <div>
              <h1 className="t-title">Trash</h1>
              <p className="t-callout">
                Notes stay here for 30 days, then they are deleted for good.
              </p>
            </div>
          </header>
          {isLoading ? (
            <SkeletonText lines={4} />
          ) : notes.length === 0 ? (
            <EmptyState
              art="trash"
              title="Trash is empty"
              tip={<ShortcutTip action="note.trash">Move the open note to Trash</ShortcutTip>}
            >
              Notes you move to Trash wait here for 30 days, so you can bring them back.
            </EmptyState>
          ) : (
            <div className="nt-list">
              <AnimatePresence initial={false}>
                {notes.map((note) => (
                  <motion.div
                    key={note.id}
                    layout="position"
                    initial={false}
                    exit={{ opacity: 0, x: reduced ? 0 : 8, transition: tweens.exit }}
                    transition={reduced ? reducedTransition : springs.smooth}
                  >
                    <ListItem
                      tabIndex={0}
                      heading={
                        <span className="nt-version-title">
                          {note.kind === "meeting" && <Icon icon={Mic} size="sm" />}
                          {displayTitle(note.title)}
                        </span>
                      }
                      meta={`${note.snippet ? `${note.snippet} · ` : ""}Deleted ${deletedWhen(note.deleted_at ?? note.updated_at)} · ${daysLeft(note)} days left`}
                      actions={
                        <>
                          <Button
                            size="sm"
                            variant="secondary"
                            icon={RotateCcw}
                            iconMotion="undo"
                            onClick={() => restore.mutate(note.id)}
                          >
                            Restore
                          </Button>
                          <IconButton
                            icon={Trash2}
                            iconMotion="trash"
                            label="Delete forever"
                            onClick={() => setConfirm({ kind: "one", note })}
                          />
                        </>
                      }
                    />
                  </motion.div>
                ))}
              </AnimatePresence>
            </div>
          )}
        </PageColumn>
      </ScrollArea>
      <ConfirmDialog
        open={confirm !== null}
        onOpenChange={(open) => !open && setConfirm(null)}
        destructive
        title={
          confirm?.kind === "one"
            ? `Delete ${displayTitle(confirm.note.title)} forever?`
            : "Empty Trash?"
        }
        body={
          confirm?.kind === "one"
            ? "The note and its history are deleted for good. This cannot be undone."
            : `${notes.length === 1 ? "1 note is" : `${notes.length} notes are`} deleted for good, with their history. This cannot be undone.`
        }
        confirmLabel={
          confirm?.kind === "one"
            ? "Delete forever"
            : `Delete ${notes.length === 1 ? "1 note" : `${notes.length} notes`}`
        }
        onConfirm={() => {
          if (confirm?.kind === "one") remove.mutate(confirm.note.id);
          else empty.mutate();
        }}
      />
    </ViewLayer>
  );
}
