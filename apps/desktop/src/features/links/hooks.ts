/** Links between notes: the graph, backlinks, and opening or creating what a link points at. */
import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect } from "react";
import { qk } from "@/app/queryKeys";
import { navigate, openNote } from "@/app/router";
import { toSummary, useNotes } from "@/features/notes/hooks";
import { usePeople } from "@/features/people/hooks";
import { api, errorMessage } from "@/ipc/client";
import type { Note, NoteSummary } from "@/ipc/types";
import { toast } from "@/lib/toast";
import { resolveLink, useLinkResolver } from "./resolver";

/** The whole graph (`focus` null) or the part within `depth` links of a note. */
export function useLinkGraph(focus: string | null, depth = 1) {
  return useQuery({
    queryKey: qk.linkGraph(focus, depth),
    queryFn: () => api.getLinkGraph(focus, depth),
    placeholderData: keepPreviousData,
    // The graph is cheap to read and should follow edits made elsewhere.
    staleTime: 0,
  });
}

export function useBacklinks(noteId: string) {
  return useQuery({
    queryKey: qk.backlinks(noteId),
    queryFn: () => api.getBacklinks(noteId),
    placeholderData: keepPreviousData,
    staleTime: 0,
  });
}

/** Turn an unlinked mention into a link, in the note that mentions it. */
export function useLinkMention() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ sourceId, targetId }: { sourceId: string; targetId: string }) =>
      api.linkMention(sourceId, targetId),
    onSuccess: (_summary, { sourceId }) => {
      void queryClient.invalidateQueries({ queryKey: qk.note(sourceId) });
      void queryClient.invalidateQueries({ queryKey: qk.links });
    },
    onError: (error) =>
      toast.error("The mention was not linked", { description: errorMessage(error) }),
  });
}

/** Keep the editor's link styling in step with the notes list and Saved people. */
export function useLinkResolverSync() {
  const { data: notes } = useNotes();
  const { data: people } = usePeople();
  useEffect(() => {
    useLinkResolver.getState().set(notes ?? [], people ?? []);
  }, [notes, people]);
}

/** A new note with this title, for a link to a note not created yet. Opens it. */
export function useCreateLinkedNote() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ title, categoryId }: { title: string; categoryId: string | null }) => {
      const note = await api.createNote(categoryId);
      const summary = await api.renameNote(note.id, title);
      return { ...note, ...summary } satisfies Note;
    },
    onSuccess: (note) => {
      queryClient.setQueryData(qk.note(note.id), note);
      queryClient.setQueryData<NoteSummary[]>(qk.notes, (list) => [
        toSummary(note),
        ...(list ?? []),
      ]);
      void queryClient.invalidateQueries({ queryKey: qk.links });
      openNote(note.id);
    },
    onError: (error) =>
      toast.error("The note was not created", { description: errorMessage(error) }),
  });
}

/** Follow a link: open the note, the person, or create the note it names. */
export function useOpenLink(categoryId: string | null = null) {
  const create = useCreateLinkedNote();
  return useCallback(
    (target: string) => {
      const resolved = resolveLink(target);
      switch (resolved.kind) {
        case "note":
          openNote(resolved.noteId);
          return;
        case "person":
          navigate({ name: "people" });
          return;
        case "missing":
          if (resolved.title) create.mutate({ title: resolved.title, categoryId });
      }
    },
    [create, categoryId],
  );
}
