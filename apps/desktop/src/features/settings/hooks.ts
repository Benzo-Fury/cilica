import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { qk } from "@/app/queryKeys";
import { api, errorMessage } from "@/ipc/client";
import type { Settings } from "@/ipc/types";
import { toast } from "@/lib/toast";

export function useSettings() {
  return useQuery({ queryKey: qk.settings, queryFn: () => api.getSettings(), staleTime: Infinity });
}

export function useAppInfo() {
  return useQuery({ queryKey: qk.appInfo, queryFn: () => api.getAppInfo(), staleTime: Infinity });
}

type Section = "appearance" | "recording" | "transcription" | "ai" | "sounds" | "swear_jar";

/**
 * Change settings optimistically: the UI updates at once, the backend gets the whole document,
 * and a failure rolls back with an error toast. Switches have no Save button behind them.
 */
export function useUpdateSettings() {
  const queryClient = useQueryClient();

  const update = useCallback(
    (recipe: (current: Settings) => Settings) => {
      const previous = queryClient.getQueryData<Settings>(qk.settings);
      if (!previous) return;
      const next = recipe(previous);
      queryClient.setQueryData(qk.settings, next);
      api.updateSettings(next).then(
        (saved) => queryClient.setQueryData(qk.settings, saved),
        (error) => {
          queryClient.setQueryData(qk.settings, previous);
          toast.error("That setting was not saved", { description: errorMessage(error) });
        },
      );
    },
    [queryClient],
  );

  const patch = useCallback(
    <S extends Section>(section: S, values: Partial<Settings[S]>) =>
      update((s) => ({ ...s, [section]: { ...s[section], ...values } })),
    [update],
  );

  return { update, patch };
}
