import { AnimatePresence } from "motion/react";
import { useEffect, useMemo, useRef } from "react";
import { openNote, useOpenNoteId } from "@/app/router";
import { useNotes } from "@/features/notes/hooks";
import { isLive, useRecordingState } from "@/features/recorder/hooks";
import { displayTitle } from "@/lib/format";
import { haptic } from "@/lib/haptics";
import { TabBar, type TabBarItem } from "@/ui";
import { closeNoteTab, keepTab, pruneTabs, reorderTabs, useTabs } from "./store";

/** The bar of open notes under the canvas. Shows only while tabs are on and a note is open in one. */
export function OpenTabs() {
  const enabled = useTabs((s) => s.enabled);
  const tabs = useTabs((s) => s.tabs);
  const preview = useTabs((s) => s.preview);
  const openId = useOpenNoteId();
  const { data: notes } = useNotes();
  const recording = useRecordingState();
  const recordingId = isLive(recording) ? recording.note_id : null;

  // Close tabs whose notes have gone (Trash, deleted). Only ids the list has shown before count as
  // gone, so a note created a moment ago is not closed before the list catches up.
  const seen = useRef(new Set<string>());
  useEffect(() => {
    if (!notes) return;
    const present = new Set(notes.map((n) => n.id));
    pruneTabs((id) => present.has(id) || !seen.current.has(id));
    for (const id of present) seen.current.add(id);
  }, [notes]);

  // Recording into a note is working on it.
  useEffect(() => {
    if (recordingId && recordingId === preview) keepTab(recordingId);
  }, [recordingId, preview]);

  const items = useMemo(() => {
    const byId = new Map((notes ?? []).map((n) => [n.id, n]));
    return tabs.flatMap((id): TabBarItem[] => {
      const note = byId.get(id);
      if (!note) return [];
      const title = displayTitle(note.title);
      return [{ id, title, recording: id === recordingId, preview: id === preview }];
    });
  }, [tabs, preview, notes, recordingId]);

  const activeId = openId && tabs.includes(openId) ? openId : null;
  return (
    <AnimatePresence initial={false}>
      {enabled && items.length > 0 && (
        <TabBar
          key="tabs"
          tabs={items}
          activeId={activeId}
          onSelect={(id) => openNote(id)}
          onClose={closeNoteTab}
          onKeep={keepTab}
          onReorder={(order) => {
            reorderTabs(order);
            // The tab snaps into its new place.
            haptic("alignment");
          }}
        />
      )}
    </AnimatePresence>
  );
}
