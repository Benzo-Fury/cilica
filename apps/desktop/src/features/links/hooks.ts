/** Links between notes: the graph, backlinks, and opening or creating what a link points at. */
import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect } from "react";
import { qk } from "@/app/queryKeys";
import { navigate, openNote } from "@/app/router";
import { toSummary, useNotes } from "@/features/notes/hooks";
import { usePeople } from "@/features/people/hooks";
import { api, errorMessage } from "@/ipc/client";
import type { LinkSuggestion, Note, NoteSummary } from "@/ipc/types";
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

/** Notes this one could link to (at most 3), worked out when the note opens. */
export function useLinkSuggestions(noteId: string, enabled = true) {
  return useQuery({
    queryKey: qk.linkSuggestions(noteId),
    queryFn: () => api.getLinkSuggestions(noteId),
    enabled,
    staleTime: 0,
  });
}

/**
 * Approve or turn down a suggested link. Either way it leaves the list at once; approving adds the
 * link to the note's hidden `related` property, so the graph and backlinks pick it up.
 */
export function useAnswerLinkSuggestion(noteId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ targetId, accept }: { targetId: string; accept: boolean }) =>
      accept
        ? api.acceptLinkSuggestion(noteId, targetId)
        : api.dismissLinkSuggestion(noteId, targetId),
    onMutate: async ({ targetId }) => {
      const key = qk.linkSuggestions(noteId);
      await queryClient.cancelQueries({ queryKey: key });
      const before = queryClient.getQueryData<LinkSuggestion[]>(key);
      queryClient.setQueryData<LinkSuggestion[]>(key, (list) =>
        list?.filter((s) => s.note.id !== targetId),
      );
      return { before };
    },
    onSuccess: (summary) => {
      queryClient.setQueryData<NoteSummary[]>(qk.notes, (list) =>
        list?.map((n) => (n.id === summary.id ? summary : n)),
      );
      // The body's properties changed; the visible text did not, so the editor stays as it is.
      void queryClient.invalidateQueries({ queryKey: qk.note(noteId) });
      void queryClient.invalidateQueries({ queryKey: qk.versions(noteId) });
      void queryClient.invalidateQueries({ queryKey: qk.links });
    },
    onError: (error, { accept }, context) => {
      queryClient.setQueryData(qk.linkSuggestions(noteId), context?.before);
      toast.error(accept ? "The link was not added" : "The suggestion was not dismissed", {
        description: errorMessage(error),
      });
    },
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
          navigate({ name: "people", personId: resolved.personId });
          return;
        case "missing":
          if (resolved.title) create.mutate({ title: resolved.title, categoryId });
      }
    },
    [create, categoryId],
  );
}
