/** Window-level commands (haptics, title bar): no-ops in the browser preview. */
import type { HapticKind } from "@/ipc/bindings";

export function windowCommands() {
  return {
    performHaptic: (_kind: HapticKind): null => null,
    titleBarDoubleClick: (): null => null,
  };
}
