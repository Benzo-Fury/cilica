import { QueryClient } from "@tanstack/react-query";
import type { AppError } from "@/ipc/types";

/** The app's query client: no refetch on focus, no retries for errors a retry cannot fix. */
export function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        refetchOnWindowFocus: false,
        retry: (count, error) =>
          count < 1 &&
          !["not_found", "not_implemented", "invalid"].includes(
            (error as unknown as AppError)?.code,
          ),
      },
      mutations: { retry: false },
    },
  });
}
