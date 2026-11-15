/** Saved people: the list and its edits. Speakers linked to a person follow their name and colour. */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { qk } from "@/app/queryKeys";
import { api, errorMessage } from "@/ipc/client";
import type { Person } from "@/ipc/types";
import { toast } from "@/lib/toast";

export function usePeople() {
  return useQuery({ queryKey: qk.people, queryFn: () => api.listPeople() });
}

/** What Cilica knows about one person's voice, while `enabled` (their row is open). */
export function usePersonVoice(personId: string, enabled = true) {
  return useQuery({
    queryKey: qk.personVoice(personId),
    queryFn: () => api.getPersonVoice(personId),
    enabled,
  });
}

/** The meetings a person was in, newest first. */
export function usePersonMeetings(personId: string, enabled = true) {
  return useQuery({
    queryKey: qk.personMeetings(personId),
    queryFn: () => api.listPersonMeetings(personId),
    enabled,
  });
}

function usePeopleCache() {
  const queryClient = useQueryClient();
  const replace = (person: Person) => {
    queryClient.setQueryData<Person[]>(qk.people, (list) =>
      list?.map((p) => (p.id === person.id ? person : p)),
    );
    void queryClient.invalidateQueries({ queryKey: qk.personVoice(person.id) });
  };
  // Linked speakers show a person's name and colour, so transcripts refresh too.
  const refreshTranscripts = () => {
    void queryClient.invalidateQueries({ queryKey: qk.transcriptPrefix });
    void queryClient.invalidateQueries({ queryKey: qk.speakerSuggestionsPrefix });
  };
  return { queryClient, replace, refreshTranscripts };
}

export function useRenamePerson() {
  const { queryClient, replace, refreshTranscripts } = usePeopleCache();
  return useMutation({
    mutationFn: ({ personId, name }: { personId: string; name: string }) =>
      api.renamePerson(personId, name),
    onMutate: ({ personId, name }) => {
      const previous = queryClient.getQueryData<Person[]>(qk.people);
      queryClient.setQueryData<Person[]>(qk.people, (list) =>
        list?.map((p) => (p.id === personId ? { ...p, name } : p)),
      );
      return { previous };
    },
    onError: (error, _v, context) => {
      queryClient.setQueryData(qk.people, context?.previous);
      toast.error("The name was not changed", { description: errorMessage(error) });
    },
    onSuccess: (person) => {
      replace(person);
      refreshTranscripts();
    },
  });
}

export function useSetPersonColor() {
  const { queryClient, replace, refreshTranscripts } = usePeopleCache();
  return useMutation({
    mutationFn: ({ personId, color }: { personId: string; color: number }) =>
      api.setPersonColor(personId, color),
    onMutate: ({ personId, color }) => {
      const previous = queryClient.getQueryData<Person[]>(qk.people);
      queryClient.setQueryData<Person[]>(qk.people, (list) =>
        list?.map((p) => (p.id === personId ? { ...p, color_index: color } : p)),
      );
      return { previous };
    },
    onError: (error, _v, context) => {
      queryClient.setQueryData(qk.people, context?.previous);
      toast.error("The colour was not changed", { description: errorMessage(error) });
    },
    onSuccess: (person) => {
      replace(person);
      refreshTranscripts();
    },
  });
}

export function useMergePeople() {
  const { queryClient, refreshTranscripts } = usePeopleCache();
  return useMutation({
    mutationFn: ({ sourceId, targetId }: { sourceId: string; targetId: string }) =>
      api.mergePeople(sourceId, targetId),
    onSuccess: (target, { sourceId }) => {
      const source = queryClient.getQueryData<Person[]>(qk.people)?.find((p) => p.id === sourceId);
      queryClient.setQueryData<Person[]>(qk.people, (list) =>
        list?.filter((p) => p.id !== sourceId).map((p) => (p.id === target.id ? target : p)),
      );
      refreshTranscripts();
      toast.success(`Merged ${source?.name ?? "them"} into ${target.name}`, {
        description: "Their voice samples and meetings now belong to one person.",
      });
    },
    onError: (error) =>
      toast.error("Those people were not merged", { description: errorMessage(error) }),
  });
}

/** Forget how a person sounds; they and their meetings stay. */
export function useForgetVoice() {
  const { replace, refreshTranscripts } = usePeopleCache();
  return useMutation({
    mutationFn: (personId: string) => api.forgetPersonVoice(personId),
    onError: (error) =>
      toast.error("The voice was not forgotten", { description: errorMessage(error) }),
    onSuccess: (person) => {
      replace(person);
      refreshTranscripts();
      toast(person.is_self ? "Forgot your voice" : `Forgot ${person.name}'s voice`);
    },
  });
}

export function useDeletePerson() {
  const { queryClient, refreshTranscripts } = usePeopleCache();
  return useMutation({
    mutationFn: (personId: string) => api.deletePerson(personId),
    onMutate: (personId) => {
      const previous = queryClient.getQueryData<Person[]>(qk.people);
      queryClient.setQueryData<Person[]>(qk.people, (list) =>
        list?.filter((p) => p.id !== personId),
      );
      return { previous };
    },
    onError: (error, _v, context) => {
      queryClient.setQueryData(qk.people, context?.previous);
      toast.error("That person was not deleted", { description: errorMessage(error) });
    },
    onSuccess: (_r, personId, context) => {
      const name = context?.previous?.find((p) => p.id === personId)?.name ?? "person";
      refreshTranscripts();
      toast(`Deleted ${name}`, { description: "Their past meetings keep the speaker's name." });
    },
  });
}

/** Take one meeting out of a person's voice; Cilica does not learn it back. */
export function useRemoveVoiceSample() {
  const { replace } = usePeopleCache();
  return useMutation({
    mutationFn: ({ sampleId }: { sampleId: string; title: string }) =>
      api.removeVoiceSample(sampleId),
    onError: (error) =>
      toast.error("Cilica is still learning from that meeting", {
        description: errorMessage(error),
      }),
    onSuccess: (person, { title }) => {
      replace(person);
      toast(`Stopped learning from “${title}”`, {
        description: "The meeting stays, and still names them.",
      });
    },
  });
}

/** Forget every voice Cilica knows. People and meetings stay. */
export function useForgetAllVoices() {
  const { queryClient, refreshTranscripts } = usePeopleCache();
  return useMutation({
    mutationFn: () => api.forgetAllVoices(),
    onError: (error) =>
      toast.error("The voices were not forgotten", { description: errorMessage(error) }),
    onSuccess: (people, _v) => {
      const known = queryClient
        .getQueryData<Person[]>(qk.people)
        ?.filter((p) => p.sample_count > 0);
      queryClient.setQueryData(qk.people, people);
      void queryClient.invalidateQueries({ queryKey: qk.people });
      refreshTranscripts();
      const n = known?.length ?? 0;
      toast(n === 1 ? "Forgot 1 voice" : `Forgot ${n} voices`);
    },
  });
}
