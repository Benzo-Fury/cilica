/** Minimise / maximise / close for Cilica's own caption buttons (Windows and Linux). */
import { useMemo } from "react";
import { isTauri } from "@/ipc/client";

async function currentWindow() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

export function useWindowControls() {
  return useMemo(
    () => ({
      onMinimize: () => {
        if (isTauri) void currentWindow().then((w) => w.minimize());
      },
      onToggleMaximize: () => {
        if (isTauri) void currentWindow().then((w) => w.toggleMaximize());
      },
      onClose: () => {
        if (isTauri) void currentWindow().then((w) => w.close());
      },
    }),
    [],
  );
}
