/**
 * Notes and categories: queries plus mutations with optimistic updates and rollback.
 * Shared by the sidebar, the note view, the palette and Trash.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { qk } from "@/app/queryKeys";
import { navigate, openNote, useRouter } from "@/app/router";
import { api, errorMessage } from "@/ipc/client";
import type { Category, Note, NoteSummary } from "@/ipc/types";
import { displayTitle } from "@/lib/format";
import { toast } from "@/lib/toast";
import { byPosition } from "./positions";

export function toSummary(note: Note): NoteSummary {
  const { body_md: _body, started_at: _s, ended_at: _e, ...summary } = note;
  return summary;
}

export function useNotes() {
  return useQuery({ queryKey: qk.notes, queryFn: () => api.listNotes() });
}

export function useCategories() {
  return useQuery({
    queryKey: qk.categories,
    queryFn: async () => byPosition(await api.listCategories()),
  });
}

export function useNote(noteId: string | null) {
  return useQuery({
    queryKey: qk.note(noteId ?? ""),
    queryFn: () => api.getNote(noteId as string),
    enabled: Boolean(noteId),
  });
}

export function useTrash() {
  return useQuery({ queryKey: qk.trash, queryFn: () => api.listTrash() });
}

/** Notes grouped for the sidebar: uncategorised first, then each category's notes by position. */
export function groupNotes(notes: readonly NoteSummary[], categories: readonly Category[]) {
  const known = new Set(categories.map((c) => c.id));
  const byCategory = new Map<string, NoteSummary[]>();
  const uncategorised: NoteSummary[] = [];
  for (const note of notes) {
    if (note.category_id && known.has(note.category_id)) {
      byCategory.set(note.category_id, [...(byCategory.get(note.category_id) ?? []), note]);
    } else {
      uncategorised.push(note);
    }
  }
  return {
    uncategorised: byPosition(uncategorised),
    byCategory: new Map([...byCategory].map(([id, list]) => [id, byPosition(list)])),
  };
}

function useNotesCache() {
  const queryClient = useQueryClient();
  const patchSummary = useCallback(
    (id: string, patch: Partial<NoteSummary>) => {
      queryClient.setQueryData<NoteSummary[]>(qk.notes, (list) =>
        list?.map((n) => (n.id === id ? { ...n, ...patch } : n)),
      );
      queryClient.setQueryData<Note>(qk.note(id), (note) => (note ? { ...note, ...patch } : note));
    },
    [queryClient],
  );
  const snapshot = useCallback(
    (id?: string) => ({
      notes: queryClient.getQueryData<NoteSummary[]>(qk.notes),
      note: id ? queryClient.getQueryData<Note>(qk.note(id)) : undefined,
    }),
    [queryClient],
  );
  const restore = useCallback(
    (id: string | undefined, snap: ReturnType<typeof snapshot>) => {
      queryClient.setQueryData(qk.notes, snap.notes);
      if (id && snap.note) queryClient.setQueryData(qk.note(id), snap.note);
    },
    [queryClient],
  );
  return { queryClient, patchSummary, snapshot, restore };
}

/** New note at the top of a category (or of Notes), opened with its title focused. */
export function useCreateNote() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (categoryId: string | null) => api.createNote(categoryId),
    onSuccess: (note) => {
      queryClient.setQueryData(qk.note(note.id), note);
      queryClient.setQueryData<NoteSummary[]>(qk.notes, (list) => [
        toSummary(note),
        ...(list ?? []),
      ]);
      openNote(note.id);
    },
    onError: (error) =>
      toast.error("The note was not created", { description: errorMessage(error) }),
  });
}

/**
 * A note the user opened and left without putting anything in it is dropped, not saved. The
 * backend checks again that it is empty, so nothing written elsewhere is ever lost.
 */
export function useDiscardEmptyNote() {
  const queryClient = useQueryClient();
  return useCallback(
    (noteId: string) => {
      api.discardEmptyNote(noteId).then(
        (removed) => {
          if (!removed) return;
          queryClient.setQueryData<NoteSummary[]>(qk.notes, (list) =>
            list?.filter((n) => n.id !== noteId),
          );
          queryClient.removeQueries({ queryKey: qk.note(noteId) });
        },
        () => undefined, // it stays in the list; nothing to tell the user
      );
    },
    [queryClient],
  );
}

export function useRenameNote() {
  const { queryClient, patchSummary, snapshot, restore } = useNotesCache();
  return useMutation({
    mutationFn: ({ noteId, title }: { noteId: string; title: string }) =>
      api.renameNote(noteId, title),
    onMutate: ({ noteId, title }) => {
      const snap = snapshot(noteId);
      patchSummary(noteId, { title });
      return snap;
    },
    onError: (error, { noteId }, snap) => {
      if (snap) restore(noteId, snap);
      toast.error("The note was not renamed", { description: errorMessage(error) });
    },
    onSuccess: (summary) => {
      queryClient.setQueryData<NoteSummary[]>(qk.notes, (list) =>
        list?.map((n) => (n.id === summary.id ? summary : n)),
      );
      // Links resolve by title.
      void queryClient.invalidateQueries({ queryKey: qk.links });
    },
  });
}

export function useMoveNote() {
  const { queryClient, patchSummary, snapshot, restore } = useNotesCache();
  return useMutation({
    mutationFn: ({
      noteId,
      categoryId,
      position,
    }: {
      noteId: string;
      categoryId: string | null;
      position: number | null;
      /** Show "Moved to …" with Undo. */
      announce?: string;
    }) => api.moveNote(noteId, categoryId, position),
    onMutate: ({ noteId, categoryId, position }) => {
      const snap = snapshot(noteId);
      const previous = snap.notes?.find((n) => n.id === noteId);
      patchSummary(noteId, { category_id: categoryId, ...(position !== null ? { position } : {}) });
      return { snap, previous };
    },
    onError: (error, { noteId }, context) => {
      if (context) restore(noteId, context.snap);
      toast.error("The note was not moved", { description: errorMessage(error) });
    },
    onSuccess: (summary, { announce }, context) => {
      queryClient.setQueryData<NoteSummary[]>(qk.notes, (list) =>
        list?.map((n) => (n.id === summary.id ? summary : n)),
      );
      const previous = context?.previous;
      if (announce && previous) {
        toast.undo(`Moved ${displayTitle(summary.title)} to ${announce}`, {
          onUndo: () => {
            api.moveNote(previous.id, previous.category_id, previous.position).then(
              () => void queryClient.invalidateQueries({ queryKey: qk.notes }),
              (error) =>
                toast.error("That move was not undone", { description: errorMessage(error) }),
            );
          },
        });
      }
    },
  });
}

/** Move to Trash with an Undo toast. Leaves the note view if it was open. */
export function useTrashNote() {
  const { queryClient, snapshot, restore } = useNotesCache();
  return useMutation({
    mutationFn: (noteId: string) => api.trashNote(noteId),
    onMutate: (noteId) => {
      const snap = snapshot(noteId);
      const list = snap.notes ?? [];
      const index = list.findIndex((n) => n.id === noteId);
      const note = list[index];
      queryClient.setQueryData<NoteSummary[]>(
        qk.notes,
        list.filter((n) => n.id !== noteId),
      );
      const view = useRouter.getState().view;
      if (view.name === "note" && view.noteId === noteId) {
        const neighbour = list[index + 1] ?? list[index - 1];
        if (neighbour && neighbour.id !== noteId) openNote(neighbour.id);
        else navigate({ name: "home" });
      }
      return { snap, note };
    },
    onError: (error, noteId, context) => {
      if (context) restore(noteId, context.snap);
      toast.error("The note was not moved to Trash", { description: errorMessage(error) });
    },
    onSuccess: (_result, noteId, context) => {
      void queryClient.invalidateQueries({ queryKey: qk.trash });
      toast.undo(`Moved ${displayTitle(context?.note?.title)} to Trash`, {
        description: "Notes stay in Trash for 30 days.",
        onUndo: () => {
          api.restoreNote(noteId).then(
            () => {
              void queryClient.invalidateQueries({ queryKey: qk.notes });
              void queryClient.invalidateQueries({ queryKey: qk.trash });
            },
            (error) =>
              toast.error("The note was not restored", { description: errorMessage(error) }),
          );
        },
      });
    },
  });
}

export function useRestoreNote() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (noteId: string) => api.restoreNote(noteId),
    onMutate: (noteId) => {
      const previous = queryClient.getQueryData<NoteSummary[]>(qk.trash);
      queryClient.setQueryData<NoteSummary[]>(qk.trash, (list) =>
        list?.filter((n) => n.id !== noteId),
      );
      return { previous };
    },
    onError: (error, _id, context) => {
      queryClient.setQueryData(qk.trash, context?.previous);
      toast.error("The note was not restored", { description: errorMessage(error) });
    },
    onSuccess: (summary) => {
      void queryClient.invalidateQueries({ queryKey: qk.notes });
      toast.success(`Restored ${displayTitle(summary.title)}`, {
        action: { label: "Open", onClick: () => openNote(summary.id) },
      });
    },
  });
}

export function useDeleteForever() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (noteId: string) => api.deleteNoteForever(noteId),
    onMutate: (noteId) => {
      const previous = queryClient.getQueryData<NoteSummary[]>(qk.trash);
      queryClient.setQueryData<NoteSummary[]>(qk.trash, (list) =>
        list?.filter((n) => n.id !== noteId),
      );
      return { previous };
    },
    onError: (error, _id, context) => {
      queryClient.setQueryData(qk.trash, context?.previous);
      toast.error("The note was not deleted", { description: errorMessage(error) });
    },
    onSuccess: () => toast("Deleted 1 note"),
  });
}

export function useEmptyTrash() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => api.emptyTrash(),
    onSuccess: (count) => {
      queryClient.setQueryData(qk.trash, []);
      toast(`Deleted ${count} ${count === 1 ? "note" : "notes"}`);
    },
    onError: (error) => toast.error("Trash was not emptied", { description: errorMessage(error) }),
  });
}

// ───────── Categories ─────────

function useCategoriesCache() {
  const queryClient = useQueryClient();
  const set = (fn: (list: Category[]) => Category[]) =>
    queryClient.setQueryData<Category[]>(qk.categories, (list) => byPosition(fn(list ?? [])));
  return { queryClient, set, get: () => queryClient.getQueryData<Category[]>(qk.categories) };
}

export function useCreateCategory() {
  const { set } = useCategoriesCache();
  return useMutation({
    mutationFn: (name: string) => api.createCategory(name),
    onSuccess: (category) => set((list) => [...list, category]),
    onError: (error) =>
      toast.error("The category was not created", { description: errorMessage(error) }),
  });
}

export function useRenameCategory() {
  const { set, get } = useCategoriesCache();
  return useMutation({
    mutationFn: ({ categoryId, name }: { categoryId: string; name: string }) =>
      api.renameCategory(categoryId, name),
    onMutate: ({ categoryId, name }) => {
      const previous = get();
      set((list) => list.map((c) => (c.id === categoryId ? { ...c, name } : c)));
      return { previous };
    },
    onError: (error, _v, context) => {
      if (context?.previous) set(() => context.previous ?? []);
      toast.error("The category was not renamed", { description: errorMessage(error) });
    },
  });
}

export function useSetCategoryCollapsed() {
  const { set, get } = useCategoriesCache();
  return useMutation({
    mutationFn: ({ categoryId, collapsed }: { categoryId: string; collapsed: boolean }) =>
      api.setCategoryCollapsed(categoryId, collapsed),
    onMutate: ({ categoryId, collapsed }) => {
      const previous = get();
      set((list) => list.map((c) => (c.id === categoryId ? { ...c, collapsed } : c)));
      return { previous };
    },
    onError: (_error, _v, context) => {
      if (context?.previous) set(() => context.previous ?? []);
    },
  });
}

export function useReorderCategory() {
  const { set, get } = useCategoriesCache();
  return useMutation({
    mutationFn: ({ categoryId, position }: { categoryId: string; position: number | null }) =>
      api.reorderCategory(categoryId, position),
    onMutate: ({ categoryId, position }) => {
      const previous = get();
      if (position !== null)
        set((list) => list.map((c) => (c.id === categoryId ? { ...c, position } : c)));
      return { previous };
    },
    onError: (error, _v, context) => {
      if (context?.previous) set(() => context.previous ?? []);
      toast.error("The category was not moved", { description: errorMessage(error) });
    },
  });
}

export function useDeleteCategory() {
  const { queryClient, set, get } = useCategoriesCache();
  return useMutation({
    mutationFn: (categoryId: string) => api.deleteCategory(categoryId),
    onMutate: (categoryId) => {
      const previous = get();
      set((list) => list.filter((c) => c.id !== categoryId));
      return { previous };
    },
    onError: (error, _v, context) => {
      if (context?.previous) set(() => context.previous ?? []);
      toast.error("The category was not deleted", { description: errorMessage(error) });
    },
    onSuccess: (_r, categoryId, context) => {
      const name = context?.previous?.find((c) => c.id === categoryId)?.name ?? "category";
      void queryClient.invalidateQueries({ queryKey: qk.notes });
      toast(`Deleted ${name}`, { description: "Its notes moved to Notes." });
    },
  });
}
