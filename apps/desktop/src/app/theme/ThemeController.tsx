/**
 * Applies appearance settings to <html>: `data-theme` (light / dark, absent for system),
 * `data-look` (a named theme over that base, see `themes.ts`), `data-headings="serif"` (a scene
 * theme with serif headings on),
 * `data-material="opaque"` (Reduce transparency or the
 * browser preview: floating glass turns opaque), the note font, the OS and `spellcheck` (Check
 * spelling; every text box that does not opt out inherits it).
 * Also mirrors `settings.keybinds` into the keybind store.
 */
import { useLayoutEffect } from "react";
import { useSettings } from "@/features/settings/hooks";
import { isTauri } from "@/ipc/client";
import { useKeybindStore } from "../keybinds";
import { os } from "../platform";
import { themeLook, themeOption } from "./themes";

export function ThemeController() {
  const { data: settings } = useSettings();
  const appearance = settings?.appearance;

  useLayoutEffect(() => {
    document.documentElement.dataset.os = os;
  }, []);

  useLayoutEffect(() => {
    const root = document.documentElement;
    const theme = themeOption(appearance?.theme);
    const look = themeLook(theme);
    if (theme.scheme) root.dataset.theme = theme.scheme;
    else root.removeAttribute("data-theme");
    if (look) root.dataset.look = look;
    else root.removeAttribute("data-look");
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

  // Scene themes set note titles and headings in the serif, unless that is turned off.
  const serifHeadings =
    themeOption(appearance?.theme).scene !== null && (appearance?.scene_headings ?? true);
  useLayoutEffect(() => {
    const root = document.documentElement;
    if (serifHeadings) root.dataset.headings = "serif";
    else root.removeAttribute("data-headings");
  }, [serifHeadings]);

  const checkSpelling = settings?.writing.check_spelling ?? true;
  useLayoutEffect(() => {
    document.documentElement.spellcheck = checkSpelling;
  }, [checkSpelling]);

  const keybinds = settings?.keybinds;
  useLayoutEffect(() => {
    useKeybindStore.getState().setOverrides(keybinds ?? {});
  }, [keybinds]);

  return null;
}
