/**
 * When a search finds nothing, the palette still answers: nobody has said it, and here is the
 * closest thing someone did say. The matching happens in the store; this words it.
 */
import { useQuery } from "@tanstack/react-query";
import { qk } from "@/app/queryKeys";
import { api } from "@/ipc/client";
import type { NearWord } from "@/ipc/types";

/** Close matches for `query`, fetched only once a search for it has come back empty. */
export function useNearWords(query: string, enabled: boolean) {
  return useQuery({
    queryKey: qk.nearWords(query),
    queryFn: () => api.nearWords(query),
    enabled: enabled && query.length > 0,
    staleTime: 30_000,
  });
}

/** ", said by Priya", ", which you said" or nothing when nobody is named. */
export function saidBy(near: Pick<NearWord, "speaker" | "is_self">): string {
  if (near.is_self) return ", which you said";
  return near.speaker ? `, said by ${near.speaker}` : "";
}

/** "In Q2 planning" for one meeting, "4 meetings" for more. */
export function whereSaid(near: Pick<NearWord, "meetings" | "note_title">): string {
  return near.meetings === 1 ? `In ${near.note_title}` : `${near.meetings} meetings`;
}
