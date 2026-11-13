/** Where Cilica lives: the right-hand panel or the floating bar (Settings › Cilica). */
import { useSettings } from "@/features/settings/hooks";
import type { CilicaPlacement } from "@/ipc/types";

export function useCilicaPlacement(): CilicaPlacement {
  const { data } = useSettings();
  return data?.appearance.cilica_placement ?? "side_panel";
}
