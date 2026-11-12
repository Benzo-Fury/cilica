/** Window-level commands (Dynamic Island, haptics): no-ops in the browser preview. */
import type { HapticKind, IslandInfo } from "@/ipc/bindings";

export function windowCommands() {
  return {
    showIsland: (): null => null,
    hideIsland: (): null => null,
    setIslandSize: (_width: number | null, _height: number | null): null => null,
    /** `?notch` previews the true-black island of a Mac with a camera notch. */
    getIslandInfo: (): IslandInfo => ({
      has_notch:
        typeof window !== "undefined" && new URLSearchParams(window.location.search).has("notch"),
    }),
    performHaptic: (_kind: HapticKind): null => null,
    titleBarDoubleClick: (): null => null,
  };
}
