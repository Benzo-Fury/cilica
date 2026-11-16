/**
 * Synonyms for a selection, from the thesaurus bundled with the app (`get_synonyms`). Answers are
 * kept for the session: a word's synonyms never change, and selecting it again should be instant.
 */
import { useEffect, useState } from "react";
import { api } from "@/ipc/client";
import type { Synonyms } from "@/ipc/types";

const cache = new Map<string, Synonyms>();
const pending = new Map<string, Promise<Synonyms>>();

function load(text: string): Promise<Synonyms> {
  const known = pending.get(text);
  if (known) return known;
  const request = api
    .getSynonyms(text)
    .then((answer) => {
      cache.set(text, answer);
      return answer;
    })
    .finally(() => pending.delete(text));
  pending.set(text, request);
  return request;
}

/** Synonyms for `text` (null while loading, or when `text` is null). A failure reads as none. */
export function useSynonyms(text: string | null): Synonyms | null {
  const key = text?.toLowerCase() ?? null;
  const [answer, setAnswer] = useState<{ key: string; synonyms: Synonyms } | null>(null);
  useEffect(() => {
    if (!key || cache.has(key)) return;
    let live = true;
    load(key).then(
      (synonyms) => live && setAnswer({ key, synonyms }),
      () => live && setAnswer({ key, synonyms: { word: "", groups: [] } }),
    );
    return () => {
      live = false;
    };
  }, [key]);
  if (!key) return null;
  return cache.get(key) ?? (answer?.key === key ? answer.synonyms : null);
}
