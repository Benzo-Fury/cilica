/**
 * App updates in the browser preview. `?update` pretends a newer version exists, so the update
 * toast and Settings rows can be previewed; otherwise Margin is up to date.
 */
import type { UpdateInfo } from "@/ipc/types";
import { appError } from "./util";

export function updatesCommands() {
  let pending: UpdateInfo | null = null;
  return {
    checkForUpdate: (): UpdateInfo | null => {
      const offered =
        typeof window !== "undefined" && new URLSearchParams(window.location.search).has("update");
      pending = offered ? { version: "0.2.0", notes: "Faster search." } : null;
      return pending;
    },
    installUpdate: (): null => {
      if (!pending)
        throw appError("invalid", "There is no update to install. Check for updates first.");
      pending = null;
      return null;
    },
    restartApp: (): null => {
      if (typeof window !== "undefined") window.location.reload();
      return null;
    },
  };
}
