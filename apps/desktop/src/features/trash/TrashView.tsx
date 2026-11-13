/**
 * Trash: notes stay 30 days. Restore, delete forever, or empty it (with a confirm dialog).
 * Select several the way the sidebar does (click, ⌘-click, Shift-click, ⌘A, Esc) to restore or
 * delete them together.
 */

import { Mic, RotateCcw, Trash2 } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { type KeyboardEvent, type MouseEvent, useEffect, useMemo, useState } from "react";
import { ShortcutTip } from "@/app/keybinds";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { isMac } from "@/app/platform";
import {
  useDeleteForever,
  useDeleteNotesForever,
  useEmptyTrash,
  useRestoreNote,
  useRestoreNotes,
  useTrash,
} from "@/features/notes/hooks";
import {
  applyClick,
  clickKind,
  countNotes,
  emptySelection,
  inOrder,
  pruneSelection,
  type Selection,
  useClickAway,
} from "@/features/notes/selection";
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
  const restoreMany = useRestoreNotes();
  const removeMany = useDeleteNotesForever();
  const [confirm, setConfirm] = useState<
    { kind: "empty" } | { kind: "one"; note: NoteSummary } | { kind: "some"; ids: string[] } | null
  >(null);

  // ── Selection (local to Trash) ──
  const [selection, setSelection] = useState<Selection>(emptySelection);
  const order = useMemo(() => notes.map((n) => n.id), [notes]);
  const selected = useMemo(() => inOrder(selection.ids, order), [selection.ids, order]);
  const chosen = new Set(selected);
  useEffect(() => {
    setSelection((s) => pruneSelection(s, new Set(order)));
  }, [order]);

  const onItemClick = (event: MouseEvent<HTMLDivElement>, id: string) => {
    // The row's own buttons act on that note alone.
    if ((event.target as HTMLElement).closest("button")) return;
    setSelection((s) => applyClick(s, id, clickKind(event, isMac), order, null));
  };
  const onItemKeyDown = (event: KeyboardEvent<HTMLDivElement>, id: string) => {
    if (event.target !== event.currentTarget) return;
    const mod = isMac ? event.metaKey : event.ctrlKey;
    if (event.key === " " || event.key === "Enter") {
      event.preventDefault();
      setSelection((s) => applyClick(s, id, event.shiftKey ? "range" : "toggle", order, null));
    } else if (mod && event.key.toLowerCase() === "a") {
      event.preventDefault();
      setSelection({ ids: order, anchor: order[0] ?? null });
    } else if (event.key === "Escape" && selection.ids.length > 0) {
      event.preventDefault();
      setSelection(emptySelection);
    }
  };
  // Clicking anywhere but a note in the list (or the buttons that act on the selection) clears it.
  useClickAway(selection.ids.length > 0, ".nt-list-item", () => setSelection(emptySelection));

  const restoreSelected = () => {
    restoreMany.mutate(selected);
    setSelection(emptySelection);
  };

  return (
    <ViewLayer>
      <CanvasBand
        start={<Crumb parts={["Trash"]} />}
        end={
          selected.length > 0 ? (
            <>
              <Button
                size="sm"
                variant="secondary"
                icon={RotateCcw}
                iconMotion="undo"
                data-keeps-selection
                onClick={restoreSelected}
              >
                Restore {countNotes(selected.length)}
              </Button>
              <Button
                variant="danger"
                size="sm"
                icon={Trash2}
                iconMotion="trash"
                data-keeps-selection
                onClick={() => setConfirm({ kind: "some", ids: selected })}
              >
                Delete {countNotes(selected.length)}
              </Button>
            </>
          ) : notes.length > 0 ? (
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
                      selected={chosen.has(note.id)}
                      aria-label={`${displayTitle(note.title)}${chosen.has(note.id) ? ", selected" : ""}`}
                      onClick={(event) => onItemClick(event, note.id)}
                      onKeyDown={(event) => onItemKeyDown(event, note.id)}
                      onMouseDown={(event) => event.shiftKey && event.preventDefault()}
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
            : confirm?.kind === "some"
              ? `Delete ${countNotes(confirm.ids.length)} forever?`
              : "Empty Trash?"
        }
        body={
          confirm?.kind === "one"
            ? "The note and its history are deleted for good. This cannot be undone."
            : confirm?.kind === "some"
              ? `${confirm.ids.length === 1 ? "The note is" : "The notes are"} deleted for good, with their history. This cannot be undone.`
              : `${notes.length === 1 ? "1 note is" : `${notes.length} notes are`} deleted for good, with their history. This cannot be undone.`
        }
        confirmLabel={
          confirm?.kind === "one"
            ? "Delete forever"
            : `Delete ${countNotes(confirm?.kind === "some" ? confirm.ids.length : notes.length)}`
        }
        onConfirm={() => {
          if (confirm?.kind === "one") remove.mutate(confirm.note.id);
          else if (confirm?.kind === "some") {
            removeMany.mutate(confirm.ids);
            setSelection(emptySelection);
          } else empty.mutate();
        }}
      />
    </ViewLayer>
  );
}
