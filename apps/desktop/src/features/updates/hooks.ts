/**
 * App updates from GitHub Releases.
 *
 * Margin checks shortly after launch and then every six hours. With automatic updates on, a new
 * version downloads and installs in the background and a toast offers to restart. With them off,
 * the toast offers to install it. Settings › General can check on demand; a version found there
 * downloads only when its Download button is pressed.
 */
import { useEffect } from "react";
import { create } from "zustand";
import { useSettings } from "@/features/settings/hooks";
import { api, errorMessage, isTauri } from "@/ipc/client";
import type { UpdateInfo } from "@/ipc/types";
import { toast } from "@/lib/toast";

const FIRST_CHECK_MS = 30_000;
const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;
const TOAST_ID = "app-update";

export type UpdatePhase =
  | "idle"
  | "checking"
  | "current"
  | "available"
  | "installing"
  | "ready"
  | "error";

interface UpdateState {
  phase: UpdatePhase;
  update: UpdateInfo | null;
  error: string | null;
}

export const useUpdateState = create<UpdateState>()(() => ({
  phase: "idle",
  update: null,
  error: null,
}));

const setState = useUpdateState.setState;

// ───────── Actions ─────────

/** Restart Margin to finish an installed update. */
export function restartToUpdate() {
  api
    .restartApp()
    .catch((error: unknown) =>
      toast.error("Margin did not restart", { description: errorMessage(error) }),
    );
}

function showReady(update: UpdateInfo) {
  toast.action(`Margin ${update.version} is ready`, {
    id: TOAST_ID,
    description: "Restart to finish updating.",
    duration: null,
    action: { label: "Restart", onClick: restartToUpdate },
  });
}

/** Download and install the update found by the last check. */
export async function installUpdate() {
  const { update, phase } = useUpdateState.getState();
  if (!update || phase === "installing" || phase === "ready") return;
  setState({ phase: "installing", error: null });
  try {
    await api.installUpdate();
    setState({ phase: "ready" });
    showReady(update);
  } catch (error) {
    const message = errorMessage(error);
    setState({ phase: "error", error: message });
    toast.error("The update did not install", { id: TOAST_ID, description: message });
  }
}

/**
 * Ask GitHub for a newer version. A background check stays quiet when nothing is found or the
 * check fails; a manual one says so.
 */
export async function checkForUpdate({
  manual,
  autoInstall,
}: {
  manual: boolean;
  autoInstall: boolean;
}) {
  const { phase } = useUpdateState.getState();
  if (phase === "checking" || phase === "installing" || phase === "ready") return;
  setState({ phase: "checking", error: null });
  try {
    const update = await api.checkForUpdate();
    if (!update) {
      setState({ phase: "current", update: null });
      return;
    }
    setState({ phase: "available", update });
    // A manual check from Settings only reports the update; its Download button installs it.
    if (manual) return;
    if (autoInstall) {
      await installUpdate();
    } else {
      toast.action(`Margin ${update.version} is available`, {
        id: TOAST_ID,
        duration: null,
        action: { label: "Install", onClick: () => void installUpdate() },
      });
    }
  } catch (error) {
    const message = errorMessage(error);
    setState({ phase: "error", error: message });
    if (manual) toast.error("Margin could not check for updates", { description: message });
  }
}

// ───────── Background checks ─────────

/**
 * Mounted once by the shell. Skipped in `bun run dev`, where the build has no release to compare
 * against; the browser preview still runs it so `?update` can be tried.
 */
export function useBackgroundUpdates() {
  const { data: settings } = useSettings();
  const ready = Boolean(settings);
  const autoInstall = settings?.updates.auto_install ?? true;

  useEffect(() => {
    if (!ready || (isTauri && import.meta.env.DEV)) return;
    const run = () => void checkForUpdate({ manual: false, autoInstall });
    const first = setTimeout(run, isTauri ? FIRST_CHECK_MS : 1_000);
    const every = setInterval(run, CHECK_EVERY_MS);
    return () => {
      clearTimeout(first);
      clearInterval(every);
    };
  }, [ready, autoInstall]);
}
