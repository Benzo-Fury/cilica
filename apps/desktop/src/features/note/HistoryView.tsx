/**
 * Version history: every snapshot with its author (You or Cilica), a prose diff of what changed
 * since that version, and one-click restore (undoable).
 */

import { History, RotateCcw, Sparkles, UserRound } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useMemo, useState } from "react";
import { ShortcutTip } from "@/app/keybinds";
import { useEscapeLayer } from "@/app/layers";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { api, errorMessage } from "@/ipc/client";
import type { Note, NoteVersion } from "@/ipc/types";
import { dayAndTime, relativeDate } from "@/lib/format";
import { diffStats, documentRows, proseDiff } from "@/lib/proseDiff";
import { toast } from "@/lib/toast";
import { Button, DiffCard, EmptyState, Icon, ListItem, ProseDiffRows, SkeletonText } from "@/ui";
import { normalizeMarkdown } from "./editor/markdown";
import { useRestoreVersion, useVersions } from "./useVersions";

export function HistoryView({
  note,
  currentBody,
  onRestored,
  onClose,
}: {
  note: Note;
  /** The editor's body right now (may be ahead of the last save). */
  currentBody: string;
  onRestored: (markdown: string) => void;
  onClose: () => void;
}) {
  const reduced = useReducedMotion();
  const { data: versions, isLoading } = useVersions(note.id);
  const restore = useRestoreVersion(note.id);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  useEscapeLayer(true, onClose);

  const selected: NoteVersion | undefined = useMemo(
    () => versions?.find((v) => v.id === selectedId) ?? versions?.[0],
    [versions, selectedId],
  );
  const current = normalizeMarkdown(currentBody);
  // A readable prose diff (headings, lists and tasks in the note's own shape, no Markdown marks).
  const rows = useMemo(
    () => (selected ? documentRows(proseDiff(normalizeMarkdown(selected.body_md), current)) : []),
    [selected, current],
  );
  const { added, removed } = diffStats(rows);
  const unchanged = rows.every((r) => r.change === "same");

  const doRestore = (version: NoteVersion) => {
    const previous = currentBody;
    restore.mutate(version.id, {
      onSuccess: (restored) => {
        onRestored(restored.body_md);
        onClose();
        toast.undo(`Restored the version from ${dayAndTime(version.created_at)}`, {
          onUndo: () => {
            api.updateNoteBody(note.id, previous).then(
              () => onRestored(previous),
              (error) =>
                toast.error("The restore was not undone", { description: errorMessage(error) }),
            );
          },
        });
      },
    });
  };

  if (isLoading) {
    return (
      <div className="nt-history">
        <div className="nt-history-list">
          <SkeletonText lines={4} />
        </div>
        <div className="nt-history-main">
          <SkeletonText lines={6} />
        </div>
      </div>
    );
  }

  if (!versions || versions.length === 0) {
    return (
      <EmptyState
        art="note"
        title="No earlier versions yet"
        tip={<ShortcutTip action="note.history">Open history from any note</ShortcutTip>}
      >
        Cilica keeps a version before every change it makes and every few minutes while you write.
      </EmptyState>
    );
  }

  return (
    <div className="nt-history">
      <div className="nt-history-list nt-list" role="listbox" aria-label="Versions">
        {versions.map((version) => {
          const isSelected = version.id === selected?.id;
          return (
            <ListItem
              key={version.id}
              role="option"
              tabIndex={0}
              aria-selected={isSelected}
              selected={isSelected}
              onClick={() => setSelectedId(version.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  setSelectedId(version.id);
                }
              }}
              heading={
                <span className="nt-version-title">
                  <Icon icon={version.author === "ai" ? Sparkles : UserRound} size="sm" />
                  {version.author === "ai" ? "Cilica" : "You"}
                </span>
              }
              meta={`${dayAndTime(version.created_at)} · ${relativeDate(version.created_at)}`}
            />
          );
        })}
      </div>
      <div className="nt-history-main">
        <AnimatePresence mode="wait" initial={false}>
          {selected && (
            <motion.div
              key={selected.id}
              initial={{ opacity: 0, y: reduced ? 0 : 4 }}
              animate={{
                opacity: 1,
                y: 0,
                transition: reduced ? reducedTransition : springs.smooth,
              }}
              exit={{ opacity: 0, transition: tweens.exit }}
            >
              <DiffCard
                head={
                  <>
                    <Icon icon={History} size="sm" />
                    <strong>{selected.author === "ai" ? "Cilica" : "You"}</strong>
                    <span>{dayAndTime(selected.created_at)}</span>
                    <span className="nt-diff-legend">
                      {unchanged ? "Same as now" : `${added} added · ${removed} removed since`}
                    </span>
                  </>
                }
                foot={
                  <>
                    <span className="nt-faint nt-diff-foot-note">
                      Restoring keeps the current text as a version.
                    </span>
                    <Button
                      variant="primary"
                      size="sm"
                      icon={RotateCcw}
                      iconMotion="undo"
                      disabled={unchanged}
                      loading={restore.isPending}
                      onClick={() => doRestore(selected)}
                    >
                      Restore this version
                    </Button>
                  </>
                }
              >
                <ProseDiffRows rows={rows} dimSame />
              </DiffCard>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}
