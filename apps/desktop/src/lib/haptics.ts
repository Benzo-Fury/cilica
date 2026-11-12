/** Trackpad haptics (macOS Force Touch only; silently nothing elsewhere). Never on hover or typing. */
import { isMac } from "@/app/platform";
import { api, isTauri } from "@/ipc/client";
import type { HapticKind } from "@/ipc/types";

export function haptic(kind: HapticKind) {
  if (!isTauri || !isMac) return;
  void api.performHaptic(kind).catch(() => undefined);
}
