/** The settings pages, in sidebar order. */

import type { LucideIcon } from "lucide-react";
import {
  AudioLines,
  Bot,
  Coins,
  FolderInput,
  Keyboard,
  Mic,
  Palette,
  Settings2,
  Volume2,
} from "lucide-react";
import type { ComponentType } from "react";
import type { SettingsPageId } from "@/app/router";
import { ImportPage } from "@/features/import/ImportPage";
import { AppearancePage } from "./pages/AppearancePage";
import { GeneralPage } from "./pages/GeneralPage";
import { KeybindsPage } from "./pages/KeybindsPage";
import { MarginPage } from "./pages/MarginPage";
import { RecordingPage } from "./pages/RecordingPage";
import { SoundsPage } from "./pages/SoundsPage";
import { SwearJarPage } from "./pages/SwearJarPage";
import { TranscriptionPage } from "./pages/TranscriptionPage";

export interface SettingsPageDef {
  id: SettingsPageId;
  label: string;
  icon: LucideIcon;
  /** Extra words the command palette matches. */
  keywords: string[];
  component: ComponentType;
}

export const SETTINGS_PAGES: readonly SettingsPageDef[] = [
  {
    id: "general",
    label: "General",
    icon: Settings2,
    keywords: ["launch", "login", "startup", "version"],
    component: GeneralPage,
  },
  {
    id: "appearance",
    label: "Appearance",
    icon: Palette,
    keywords: ["theme", "dark", "light", "accent", "colour", "color", "font", "transparency"],
    component: AppearancePage,
  },
  {
    id: "keybinds",
    label: "Keybinds",
    icon: Keyboard,
    keywords: ["shortcuts", "keyboard", "hotkey"],
    component: KeybindsPage,
  },
  {
    id: "recording",
    label: "Recording",
    icon: Mic,
    keywords: ["meeting", "detect", "listening", "audio", "microphone"],
    component: RecordingPage,
  },
  {
    id: "transcription",
    label: "Transcription",
    icon: AudioLines,
    keywords: ["whisper", "speech", "language", "model"],
    component: TranscriptionPage,
  },
  {
    id: "margin",
    label: "AI",
    icon: Bot,
    keywords: ["assistant", "provider", "anthropic", "api key", "model"],
    component: MarginPage,
  },
  {
    id: "sounds",
    label: "Sounds",
    icon: Volume2,
    keywords: ["chime", "audio", "notification"],
    component: SoundsPage,
  },
];

/** Pages about what Margin has kept, listed under "Your data". */
export const DATA_PAGES: readonly SettingsPageDef[] = [
  {
    id: "import",
    label: "Import",
    icon: FolderInput,
    keywords: [
      "import",
      "migrate",
      "switch",
      "obsidian",
      "notion",
      "evernote",
      "apple notes",
      "granola",
      "otter",
      "fireflies",
      "fathom",
      "transcripts",
    ],
    component: ImportPage,
  },
  {
    id: "swear_jar",
    label: "Swear jar",
    icon: Coins,
    keywords: ["swearing", "swears", "profanity", "coins", "jar"],
    component: SwearJarPage,
  },
];

/** Every settings page, for the command palette and lookups. */
export const ALL_SETTINGS_PAGES: readonly SettingsPageDef[] = [...SETTINGS_PAGES, ...DATA_PAGES];

export function settingsPage(id: SettingsPageId): SettingsPageDef {
  return ALL_SETTINGS_PAGES.find((p) => p.id === id) ?? (SETTINGS_PAGES[0] as SettingsPageDef);
}
