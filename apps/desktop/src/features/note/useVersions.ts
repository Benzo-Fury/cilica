import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { qk } from "@/app/queryKeys";
import { api, errorMessage } from "@/ipc/client";
import type { Note, NoteSummary } from "@/ipc/types";
import { toast } from "@/lib/toast";
import { toSummary } from "../notes/hooks";

export function useVersions(noteId: string) {
  return useQuery({ queryKey: qk.versions(noteId), queryFn: () => api.listNoteVersions(noteId) });
}

/** Restore a version (the backend snapshots the current body first). */
export function useRestoreVersion(noteId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (versionId: string) => api.restoreNoteVersion(versionId),
    onSuccess: (note: Note) => {
      queryClient.setQueryData(qk.note(noteId), note);
      queryClient.setQueryData<NoteSummary[]>(qk.notes, (list) =>
        list?.map((n) => (n.id === note.id ? toSummary(note) : n)),
      );
      void queryClient.invalidateQueries({ queryKey: qk.versions(noteId) });
    },
    onError: (error) =>
      toast.error("That version was not restored", { description: errorMessage(error) }),
  });
}
