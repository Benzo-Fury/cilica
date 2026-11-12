/** Where Margin lives: the right-hand panel or the floating bar (Settings › Margin). */
import { useSettings } from "@/features/settings/hooks";
import type { MarginPlacement } from "@/ipc/types";

export function useMarginPlacement(): MarginPlacement {
  const { data } = useSettings();
  return data?.appearance.margin_placement ?? "side_panel";
}
