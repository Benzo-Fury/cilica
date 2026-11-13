/**
 * The themes Settings › Appearance offers. Light and Dark are the two bases; every other theme is
 * a "look": a base plus the tokens it changes (`color.looks` in docs/design-system/tokens.json)
 * and, for most, a pixel scene behind the window.
 */
import type { ThemeMode } from "@/ipc/types";
import type { SceneKind } from "@/ui";

export interface ThemeOption {
  value: ThemeMode;
  label: string;
  /** The base it is built on. `null` follows the system. */
  scheme: "light" | "dark" | null;
  /** The art behind the window, if any. */
  scene: SceneKind | null;
}

export const THEMES: readonly ThemeOption[] = [
  { value: "system", label: "System", scheme: null, scene: null },
  { value: "light", label: "Light", scheme: "light", scene: null },
  { value: "dark", label: "Dark", scheme: "dark", scene: null },
  { value: "ridgeline_dawn", label: "Ridgeline dawn", scheme: "light", scene: "ridgeline" },
  { value: "ridgeline_night", label: "Ridgeline night", scheme: "dark", scene: "ridgeline" },
  { value: "tide", label: "Tide", scheme: "light", scene: "ridgeline" },
  { value: "dusk", label: "Dusk", scheme: "dark", scene: "ridgeline" },
  { value: "pines", label: "Pines", scheme: "light", scene: "pines" },
  { value: "mesa", label: "Mesa", scheme: "light", scene: "mesa" },
];

const SYSTEM = THEMES[0] as ThemeOption;

export function themeOption(value: ThemeMode | undefined): ThemeOption {
  return THEMES.find((theme) => theme.value === value) ?? SYSTEM;
}

/** The `data-look` a theme sets on `<html>`: its own name, or none for System, Light and Dark. */
export function themeLook(theme: ThemeOption): string | undefined {
  return theme.value === "system" || theme.value === theme.scheme ? undefined : theme.value;
}
