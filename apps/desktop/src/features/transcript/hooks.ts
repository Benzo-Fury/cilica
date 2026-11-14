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
import { hasUnknownSpeaker, nextPersonLabel, upsertSegment } from "./model";

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
    onSuccess: (speaker, { speakerId, personId }) => {
      replaceSpeaker(speaker);
      refresh();
      // Someone else here was already that person: the two are one speaker now.
      if (speaker.id !== speakerId) {
        void queryClient.invalidateQueries({ queryKey: qk.transcript(noteId) });
      }
      if (personId) {
        const person = queryClient
          .getQueryData<Person[]>(qk.people)
          ?.find((p) => p.id === personId);
        toast(`This speaker is now ${person?.name ?? speaker.label}`, {
          description:
            "Everything they said in this meeting is theirs, and Cilica will recognise them next time.",
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
        description: "Cilica will suggest them when it hears their voice again.",
      });
    },
    onError: (error) =>
      toast.error("That person was not saved", { description: errorMessage(error) }),
  });
  /** Take the person off this speaker; they go back to being "Person N" in this meeting. */
  const unlink = useMutation({
    mutationFn: async (speakerId: string) => {
      const speakers = queryClient.getQueryData<Transcript>(qk.transcript(noteId))?.speakers ?? [];
      const unlinked = await api.linkSpeaker(speakerId, null);
      const label = nextPersonLabel(speakers, speakerId);
      return unlinked.label === label ? unlinked : api.renameSpeaker(speakerId, label);
    },
    onSuccess: (speaker) => {
      replaceSpeaker(speaker);
      refresh();
      toast(`Unlinked, now ${speaker.label}`);
    },
    onError: (error) =>
      toast.error("The speaker was not unlinked", { description: errorMessage(error) }),
  });
  /** "This speaker is that one": every line moves to `into`, and this speaker goes. */
  const merge = useMutation({
    mutationFn: ({ from, into }: { from: string; into: string; name: string }) =>
      api.mergeSpeakers(from, into),
    onSuccess: (_speaker, { name }) => {
      void queryClient.invalidateQueries({ queryKey: qk.transcript(noteId) });
      refresh();
      toast(`Merged into ${name}`, {
        description: "Everything they said in this meeting is theirs now.",
      });
    },
    onError: (error) =>
      toast.error("The speakers were not merged", { description: errorMessage(error) }),
  });
  return { rename, link, save, unlink, merge };
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
