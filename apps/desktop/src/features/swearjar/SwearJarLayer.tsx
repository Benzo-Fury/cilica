/** The swear jar in the canvas corner, and the watchers that fill it. Mounted once, in the canvas. */
import { useCallback } from "react";
import { useShallow } from "zustand/react/shallow";
import { useSettings } from "@/features/settings/hooks";
import { type SwearJarHandle, SwearJarPopup } from "@/ui";
import { useSwearJarWatcher } from "./hooks";
import { registerJar, useJar } from "./jar";

export function SwearJarLayer() {
  useSwearJarWatcher();
  const { data: settings } = useSettings();
  const jar = useJar(
    useShallow((s) => ({ seen: s.seen, streak: s.streak, shown: s.shown, away: s.away })),
  );
  const ref = useCallback((handle: SwearJarHandle | null) => registerJar(handle), []);
  if (!settings?.swear_jar.enabled || !settings.swear_jar.popup) return null;
  return (
    <SwearJarPopup
      ref={ref}
      count={jar.seen ?? 0}
      streak={jar.streak}
      shown={jar.shown}
      away={jar.away}
    />
  );
}
