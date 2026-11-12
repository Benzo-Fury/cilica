import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { qk } from "@/app/queryKeys";
import { api } from "@/ipc/client";

/** Full-text search over titles, bodies and transcripts, debounced while typing. */
export function useSearchNotes(query: string, limit = 30) {
  const [debounced, setDebounced] = useState(query.trim());
  useEffect(() => {
    const t = setTimeout(() => setDebounced(query.trim()), 120);
    return () => clearTimeout(t);
  }, [query]);
  return useQuery({
    queryKey: qk.search(debounced),
    queryFn: () => api.searchNotes(debounced, limit),
    enabled: debounced.length > 0,
    placeholderData: keepPreviousData,
    staleTime: 10_000,
  });
}
