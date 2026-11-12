import { useMemo } from "react";
import { isMac } from "../platform";
import { type ActionId, effectiveKeys, formatAccelerator, getAction } from "./registry";
import { useKeybindStore } from "./store";

/** The current keycaps for an action (after overrides), e.g. ["⌘", "K"]. Empty when unbound. */
export function useShortcut(id: ActionId): string[] {
  const overrides = useKeybindStore((s) => s.overrides);
  return useMemo(() => {
    const action = getAction(id);
    return action ? formatAccelerator(effectiveKeys(action, overrides), isMac) : [];
  }, [id, overrides]);
}
