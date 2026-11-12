/**
 * Proposals (diff and confirm cards) shared by the assistant and task reports: their live status,
 * Apply (whole, or the accepted hunks of an edit as `final_md`) and Dismiss. A `conflict` means
 * the note changed after Margin suggested the edit; the card says so and stays pending.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { forgetAiUndo, rememberAiUndo } from "@/app/aiUndo";
import { qk } from "@/app/queryKeys";
import { api, errorMessage, toAppError } from "@/ipc/client";
import type { AppError, Proposal } from "@/ipc/types";
import { toast } from "@/lib/toast";

export function useProposal(proposalId: string, initial?: Proposal) {
  return useQuery({
    queryKey: qk.proposal(proposalId),
    queryFn: () => api.getProposal(proposalId),
    initialData: initial,
    // A card from the stream is fresh; refetch only when proposals change.
    staleTime: initial ? 30_000 : 0,
  });
}

export function isConflict(error: unknown): boolean {
  return (error as AppError | null)?.code === "conflict";
}

/** After an apply: offer one Undo for the action it made (the backend raises none for applies). */
async function offerUndo(proposal: Proposal) {
  try {
    const actions = await api.listAiActions(25, 0);
    const action = actions.find(
      (a) => a.proposal_id === proposal.id && a.can_undo && a.status === "applied",
    );
    if (!action) return;
    const run = () =>
      api.undoAiAction(action.id).then(
        () => toast("Undid Margin's change"),
        (error) => toast.error("That change was not undone", { description: errorMessage(error) }),
      );
    rememberAiUndo(action.summary, run);
    toast.undo(action.summary || "Applied Margin's change", {
      onUndo: () => {
        forgetAiUndo(run);
        void run();
      },
    });
  } catch {
    // The change is applied; without the action id there is simply no Undo toast.
  }
}

export function useApplyProposal(options: { undoToast?: boolean } = {}) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ proposalId, finalMd }: { proposalId: string; finalMd: string | null }) =>
      api.applyProposal(proposalId, finalMd),
    onSuccess: (proposal) => {
      queryClient.setQueryData(qk.proposal(proposal.id), proposal);
      void queryClient.invalidateQueries({ queryKey: qk.tasks });
      if (options.undoToast !== false) void offerUndo(proposal);
    },
    onError: (error) => {
      if (isConflict(error)) return; // the card explains and stays pending
      toast.error("That change was not applied", { description: toAppError(error).message });
    },
  });
}

export function useDismissProposal() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (proposalId: string) => api.dismissProposal(proposalId),
    onSuccess: (proposal) => {
      queryClient.setQueryData(qk.proposal(proposal.id), proposal);
      void queryClient.invalidateQueries({ queryKey: qk.tasks });
    },
    onError: (error) =>
      toast.error("That suggestion is still open", { description: errorMessage(error) }),
  });
}
