/**
 * A note's transcript: the query, live segment upserts, speakers (rename, link, save as a
 * person), voiceprint suggestions and the recording's audio source.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef } from "react";
import { create } from "zustand";
import { useBackendEvent } from "@/app/backendEvents";
import { qk } from "@/app/queryKeys";
import { api, errorMessage, isTauri } from "@/ipc/client";
import type { Person, Speaker, Transcript } from "@/ipc/types";
import { toast } from "@/lib/toast";
import { hasUnknownSpeaker, upsertSegment } from "./model";

/** A note's transcript (speakers and segments). Live segments are folded in by `useLiveTranscripts`. */
export function useTranscript(noteId: string | null, enabled = true) {
  return useQuery({
    queryKey: qk.transcript(noteId ?? ""),
    queryFn: () => api.getTranscript(noteId as string),
    enabled: Boolean(noteId) && enabled,
  });
}

/**
 * Mounted once: folds `transcript_segment` events into any cached transcript, so the panel
 * stays live even while the Notes side is showing. Unknown speakers trigger one refetch.
 */
export function useLiveTranscripts() {
  const queryClient = useQueryClient();
  const refetching = useRef(new Set<string>());
  useBackendEvent("transcript_segment", ({ segment }) => {
    const key = qk.transcript(segment.note_id);
    const current = queryClient.getQueryData<Transcript>(key);
    if (!current) return;
    queryClient.setQueryData<Transcript>(key, upsertSegment(current, segment));
    if (hasUnknownSpeaker(current, segment) && !refetching.current.has(segment.note_id)) {
      refetching.current.add(segment.note_id);
      void queryClient.invalidateQueries({ queryKey: key }).finally(() => {
        refetching.current.delete(segment.note_id);
      });
    }
  });
}

export function useSpeakerSuggestions(noteId: string, enabled: boolean) {
  return useQuery({
    queryKey: qk.speakerSuggestions(noteId),
    queryFn: () => api.listSpeakerSuggestions(noteId),
    enabled,
  });
}

/** "No" answers to suggestions, remembered for this session. */
export const useDeclinedSuggestions = create<{
  declined: ReadonlySet<string>;
  decline: (key: string) => void;
}>()((set) => ({
  declined: new Set(),
  decline: (key) => set((s) => ({ declined: new Set([...s.declined, key]) })),
}));

function useSpeakerCache(noteId: string) {
  const queryClient = useQueryClient();
  const replaceSpeaker = (speaker: Speaker) =>
    queryClient.setQueryData<Transcript>(qk.transcript(noteId), (t) =>
      t ? { ...t, speakers: t.speakers.map((s) => (s.id === speaker.id ? speaker : s)) } : t,
    );
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: qk.people });
    void queryClient.invalidateQueries({ queryKey: qk.speakerSuggestions(noteId) });
  };
  return { queryClient, replaceSpeaker, refresh };
}

export function useSpeakerActions(noteId: string) {
  const { queryClient, replaceSpeaker, refresh } = useSpeakerCache(noteId);
  const rename = useMutation({
    mutationFn: ({ speakerId, label }: { speakerId: string; label: string }) =>
      api.renameSpeaker(speakerId, label),
    onSuccess: (speaker) => replaceSpeaker(speaker),
    onError: (error) =>
      toast.error("The speaker was not renamed", { description: errorMessage(error) }),
  });
  const link = useMutation({
    mutationFn: ({ speakerId, personId }: { speakerId: string; personId: string | null }) =>
      api.linkSpeaker(speakerId, personId),
    onSuccess: (speaker, { personId }) => {
      replaceSpeaker(speaker);
      refresh();
      if (personId) {
        const person = queryClient
          .getQueryData<Person[]>(qk.people)
          ?.find((p) => p.id === personId);
        toast(`Linked to ${person?.name ?? speaker.label}`, {
          description: "Margin will use this meeting to recognise them next time.",
        });
      }
    },
    onError: (error) =>
      toast.error("The speaker was not linked", { description: errorMessage(error) }),
  });
  const save = useMutation({
    mutationFn: ({ speakerId, name }: { speakerId: string; name: string }) =>
      api.saveSpeakerAsPerson(speakerId, name),
    onSuccess: (person) => {
      void queryClient.invalidateQueries({ queryKey: qk.transcript(noteId) });
      refresh();
      toast.success(`Saved ${person.name}`, {
        description: "Margin will suggest them when it hears their voice again.",
      });
    },
    onError: (error) =>
      toast.error("That person was not saved", { description: errorMessage(error) }),
  });
  return { rename, link, save };
}

/**
 * Where the recording can be played from: the asset URL inside Tauri, or the mock's own
 * source in the browser preview. `null` when there is no audio any more.
 */
export function useAudioSource(noteId: string, hasAudio: boolean) {
  return useQuery({
    queryKey: qk.audioPath(noteId),
    enabled: hasAudio,
    queryFn: async () => {
      const path = await api.getAudioPath(noteId);
      if (!path) return null;
      if (!isTauri) return path;
      const { convertFileSrc } = await import("@tauri-apps/api/core");
      return convertFileSrc(path);
    },
  });
}
