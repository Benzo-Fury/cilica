/**
 * Applies appearance settings to <html>: `data-theme` (light / dark, absent for system),
 * `data-material="opaque"` (Reduce transparency or the
 * browser preview: floating glass turns opaque), the note font and the OS.
 * Also mirrors `settings.keybinds` into the keybind store.
 */
import { useLayoutEffect } from "react";
import { useSettings } from "@/features/settings/hooks";
import { isTauri } from "@/ipc/client";
import { useKeybindStore } from "../keybinds";
import { os } from "../platform";

export function ThemeController() {
  const { data: settings } = useSettings();
  const appearance = settings?.appearance;

  useLayoutEffect(() => {
    document.documentElement.dataset.os = os;
  }, []);

  useLayoutEffect(() => {
    const root = document.documentElement;
    const theme = appearance?.theme ?? "system";
    if (theme === "system") root.removeAttribute("data-theme");
    else root.dataset.theme = theme;
  }, [appearance?.theme]);

  useLayoutEffect(() => {
    const root = document.documentElement;
    const opaque = !isTauri || appearance?.transparency === false;
    if (opaque) root.dataset.material = "opaque";
    else root.removeAttribute("data-material");
  }, [appearance?.transparency]);

  useLayoutEffect(() => {
    document.documentElement.dataset.noteFont = appearance?.note_font ?? "sans";
  }, [appearance?.note_font]);

  const keybinds = settings?.keybinds;
  useLayoutEffect(() => {
    useKeybindStore.getState().setOverrides(keybinds ?? {});
  }, [keybinds]);

  return null;
}
