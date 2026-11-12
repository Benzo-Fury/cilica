import { useMutation } from "@tanstack/react-query";
import { api, errorMessage } from "@/ipc/client";
import { toast } from "@/lib/toast";

/** Settings › General › Run setup again: opens the setup window over Margin. */
export function useOpenSetup() {
  return useMutation({
    mutationFn: () => api.openOnboarding(),
    onError: (error) => toast.error("Setup did not open", { description: errorMessage(error) }),
  });
}
