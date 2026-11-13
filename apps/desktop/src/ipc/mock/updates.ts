/**
 * App updates in the browser preview. `?update` pretends a newer version exists, so the update
 * toast and Settings rows can be previewed; otherwise Cilica is up to date.
 */
import type { UpdateInfo } from "@/ipc/types";
import type { MockContext } from "./state";
import { appError } from "./util";

const MOCK_SIZE = 18_400_000;
const MOCK_STEPS = 20;
const MOCK_STEP_MS = 150;

export function updatesCommands({ emit }: Pick<MockContext, "emit">) {
  let pending: UpdateInfo | null = null;
  return {
    checkForUpdate: (): UpdateInfo | null => {
      const offered =
        typeof window !== "undefined" && new URLSearchParams(window.location.search).has("update");
      pending = offered ? { version: "0.2.0", notes: "Faster search." } : null;
      return pending;
    },
    installUpdate: async (): Promise<null> => {
      if (!pending)
        throw appError("invalid", "There is no update to install. Check for updates first.");
      pending = null;
      // Pretend to download, so the progress bar can be previewed
      for (let step = 1; step <= MOCK_STEPS; step++) {
        await new Promise((resolve) => setTimeout(resolve, MOCK_STEP_MS));
        emit({
          type: "update_download",
          downloaded_bytes: (MOCK_SIZE * step) / MOCK_STEPS,
          total_bytes: MOCK_SIZE,
        });
      }
      return null;
    },
    restartApp: (): null => {
      if (typeof window !== "undefined") window.location.reload();
      return null;
    },
  };
}
