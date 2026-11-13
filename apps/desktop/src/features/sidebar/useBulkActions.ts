/**
 * What can be done to the notes selected in the sidebar: move them, move them to Trash, or ask
 * Cilica about them. Shared by the row context menu, ⌘⌫ and drag and drop.
 */
import { useCallback, useMemo } from "react";
import { askCilica } from "@/features/assistant/store";
import {
  groupNotes,
  useCategories,
  useMoveNotes,
  useNotes,
  useTrashNotes,
} from "@/features/notes/hooks";
import { inOrder, useNoteSelection, visibleOrder } from "@/features/notes/selection";
import { displayTitle } from "@/lib/format";
import { haptic } from "@/lib/haptics";

/** The ids the sidebar shows, top to bottom. */
export function useSidebarOrder(): string[] {
  const { data: notes = [] } = useNotes();
  const { data: categories = [] } = useCategories();
  return useMemo(
    () => visibleOrder(groupNotes(notes, categories), categories),
    [notes, categories],
  );
}

export function useBulkActions() {
  const ids = useNoteSelection((s) => s.ids);
  const set = useNoteSelection((s) => s.set);
  const { data: notes = [] } = useNotes();
  const order = useSidebarOrder();
  const moveNotes = useMoveNotes();
  const trashNotes = useTrashNotes();

  // Act on the selection in sidebar order, so a moved block keeps the order it had.
  const ordered = useMemo(() => inOrder(ids, order), [ids, order]);

  /** Nothing selected; the open note stays open. */
  const clear = useCallback(() => {
    if (useNoteSelection.getState().ids.length > 0) set({ ids: [], anchor: null });
  }, [set]);

  const moveTo = useCallback(
    (categoryId: string | null, name: string) => {
      if (ordered.length === 0) return;
      haptic("alignment");
      moveNotes.mutate({ noteIds: ordered, categoryId, position: null, announce: name });
    },
    [ordered, moveNotes],
  );

  const trash = useCallback(() => {
    if (ordered.length === 0) return;
    trashNotes.mutate(ordered);
    set({ ids: [], anchor: null });
  }, [ordered, trashNotes, set]);

  const ask = useCallback(() => {
    const titles = ordered
      .map((id) => notes.find((n) => n.id === id))
      .filter((n) => n !== undefined)
      .map((n) => `[[${displayTitle(n.title)}]]`);
    if (titles.length === 0) return;
    const list =
      titles.length === 1
        ? titles[0]
        : `${titles.slice(0, -1).join(", ")} and ${titles[titles.length - 1]}`;
    askCilica(`About ${list}: `);
  }, [ordered, notes]);

  /** The one category every selected note is in (null: all in Notes), or undefined if mixed. */
  const sharedCategory = useMemo(() => {
    const cats = new Set(ordered.map((id) => notes.find((n) => n.id === id)?.category_id ?? null));
    return cats.size === 1 ? [...cats][0] : undefined;
  }, [ordered, notes]);

  return { ids: ordered, count: ordered.length, clear, moveTo, trash, ask, sharedCategory };
}
