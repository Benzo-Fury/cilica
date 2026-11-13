/**
 * Data for the command palette's search: structured note search, conversations with Cilica, and
 * the vocabulary the query parser recognises (people, categories, tags, apps, sources, titles).
 */
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { qk } from "@/app/queryKeys";
import { useAssistantStore } from "@/features/assistant/store";
import { PROVIDERS } from "@/features/import/providers";
import { useCategories, useNotes } from "@/features/notes/hooks";
import { usePeople } from "@/features/people/hooks";
import { useSettings } from "@/features/settings/hooks";
import { DEFAULT_MEETING_APPS } from "@/features/settings/pages/RecordingPage";
import { api } from "@/ipc/client";
import type { NoteQuery } from "@/ipc/types";
import { localeIsDayFirst, type SearchVocabulary } from "./query";

/** Wait this long after the last keystroke before searching. */
const DEBOUNCE_MS = 120;

/** `value` once it has stopped changing for a moment. */
function useDebounced(value: string): string {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [value]);
  return debounced;
}

/** Notes matching a parsed search, debounced while typing. `null` searches nothing. */
export function useFindNotes(query: NoteQuery | null, limit = 30) {
  // Debounce the query's text form: a new object with the same contents is not a change.
  const debounced = useDebounced(query ? JSON.stringify(query) : "");
  return useQuery({
    queryKey: qk.findNotes(`${limit}:${debounced}`),
    queryFn: () => api.findNotes(JSON.parse(debounced) as NoteQuery, limit),
    enabled: debounced !== "",
    placeholderData: keepPreviousData,
    staleTime: 10_000,
  });
}

/** Conversations with Cilica that mention every word, debounced while typing. */
export function useSearchThreads(text: string, limit = 4) {
  const trimmed = text.trim();
  const debounced = useDebounced(trimmed);
  // Conversations deleted while their Undo is still up are not offered.
  const hiddenIds = useAssistantStore((s) => s.hidden);
  const hidden = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const query = useQuery({
    queryKey: qk.searchThreads(debounced),
    queryFn: () => api.searchThreads(debounced, limit + 4),
    enabled: debounced.length > 0,
    placeholderData: keepPreviousData,
    staleTime: 10_000,
  });
  const data = useMemo(
    () =>
      debounced ? (query.data ?? []).filter((h) => !hidden.has(h.thread.id)).slice(0, limit) : [],
    [query.data, hidden, limit, debounced],
  );
  return { ...query, data };
}

/** Tags in use, apps meetings were recorded in, sources notes came from. */
export function useSearchFacets() {
  return useQuery({
    queryKey: qk.searchFacets,
    queryFn: () => api.searchFacets(),
    staleTime: 30_000,
  });
}

/** Everything the query parser recognises, with "now" fixed for as long as the palette is open. */
export function useSearchVocabulary(): SearchVocabulary {
  const [now] = useState(() => new Date());
  const { data: people = [] } = usePeople();
  const { data: categories = [] } = useCategories();
  const { data: notes = [] } = useNotes();
  const { data: facets } = useSearchFacets();
  const { data: settings } = useSettings();
  return useMemo(() => {
    const userLocale = typeof navigator !== "undefined" ? navigator.language : undefined;
    const apps = [
      ...(facets?.meeting_apps ?? []),
      ...(settings?.recording.meeting_apps ?? []),
      ...DEFAULT_MEETING_APPS,
    ];
    const seen = new Set<string>();
    return {
      now,
      people: people.map((p) => ({ id: p.id, name: p.name, is_self: p.is_self })),
      categories: categories.map((c) => ({ id: c.id, name: c.name })),
      tags: facets?.tags ?? [],
      meetingApps: apps.filter((a) => {
        const key = a.toLowerCase();
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      }),
      importProviders: PROVIDERS.filter((p) => p.id !== "files").map((p) => ({
        id: p.id,
        name: p.name,
      })),
      noteTitles: notes.map((n) => n.title).filter((t) => t.trim().length > 0),
      // Typed dates follow the computer's locale; labels follow the app's own (en-GB) voice.
      dayFirst: localeIsDayFirst(userLocale),
      locale: "en-GB",
    };
  }, [now, people, categories, notes, facets, settings]);
}
