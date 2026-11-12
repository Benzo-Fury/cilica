import type { ReactNode } from "react";
import { EmptyTip } from "@/ui";
import type { ActionId } from "./registry";
import { useShortcut } from "./useShortcut";

/** "Teach one thing": a trick and the keys for it, following the user's own keybinds. */
export function ShortcutTip({ action, children }: { action: ActionId; children: ReactNode }) {
  return <EmptyTip keys={useShortcut(action)}>{children}</EmptyTip>;
}
