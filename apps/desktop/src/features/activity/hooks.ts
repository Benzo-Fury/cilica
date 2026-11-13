/** The audit log: every tool call Cilica made, newest first, fetched a page at a time. */
import {
  type InfiniteData,
  useInfiniteQuery,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import { qk } from "@/app/queryKeys";
import { api, errorMessage } from "@/ipc/client";
import type { AiAction } from "@/ipc/types";
import { toast } from "@/lib/toast";

export const PAGE = 50;

export function useAiActions() {
  return useInfiniteQuery({
    queryKey: qk.aiActions,
    queryFn: ({ pageParam }) => api.listAiActions(PAGE, pageParam),
    initialPageParam: 0,
    getNextPageParam: (last, pages) => (last.length < PAGE ? undefined : pages.length * PAGE),
  });
}

export function useUndoAction() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (actionId: string) => api.undoAiAction(actionId),
    onSuccess: (action) => {
      queryClient.setQueryData<InfiniteData<AiAction[], number>>(qk.aiActions, (data) =>
        data
          ? {
              ...data,
              pages: data.pages.map((page) => page.map((a) => (a.id === action.id ? action : a))),
            }
          : data,
      );
      toast(`Undid: ${action.summary}`);
    },
    onError: (error) =>
      toast.error("That change was not undone", { description: errorMessage(error) }),
  });
}
