import { Monitor, Moon, Sun } from "lucide-react";
import { ACCENT_LABELS, ACCENT_PRESETS } from "@/app/theme/accent";
import { HomeFacePreview } from "@/features/home/HomeFacePreview";
import type { HomeFace, NoteFont, ThemeMode } from "@/ipc/types";
import {
  SegmentedControl,
  SelectMenu,
  SettingsGroup,
  SettingsPage,
  SettingsRow,
  Swatch,
  Switch,
} from "@/ui";
import { useSettings, useUpdateSettings } from "../hooks";

const THEMES = [
  { value: "system", label: "System", icon: Monitor },
  { value: "light", label: "Light", icon: Sun },
  { value: "dark", label: "Dark", icon: Moon },
] as const satisfies readonly { value: ThemeMode; label: string; icon: typeof Sun }[];

const FONTS = [
  { value: "sans", label: "Sans" },
  { value: "serif", label: "Serif" },
  { value: "mono", label: "Mono" },
] as const satisfies readonly { value: NoteFont; label: string }[];

const HOME_FACES = [
  { value: "random", label: "Random" },
  { value: "tics", label: "Verbal tics" },
  { value: "overheard", label: "Overheard" },
  { value: "talk", label: "Talk share" },
  { value: "pace", label: "Pace" },
  { value: "silence", label: "Longest silence" },
  { value: "own_words", label: "In your own words" },
  { value: "writing", label: "When you write" },
  { value: "questions", label: "Questions to yourself" },
  { value: "overtime", label: "Overtime" },
  { value: "slow_decision", label: "Slowest decision" },
  { value: "monologue", label: "Monologue" },
  { value: "bare_note", label: "That's the whole note" },
  { value: "mid_sentence", label: "Mid-sentence" },
  { value: "spelling", label: "Definately" },
  { value: "ghostwriter", label: "Ghostwriter" },
  { value: "undos", label: "Undo, undo, undo" },
  { value: "swear_jar", label: "The jar" },
  { value: "whos_paying", label: "Who's paying" },
  { value: "clean_week", label: "Clean week" },
  { value: "doodle", label: "Doodle" },
] as const satisfies readonly { value: HomeFace; label: string }[];

export function AppearancePage() {
  const { data: settings } = useSettings();
  const { patch } = useUpdateSettings();
  if (!settings) return null;
  const a = settings.appearance;
  return (
    <SettingsPage title="Appearance" description="How Margin looks on this device.">
      <SettingsGroup>
        <SettingsRow
          label="Theme"
          description="System follows your computer's light and dark setting."
          control={
            <SegmentedControl
              label="Theme"
              options={THEMES}
              value={a.theme}
              onChange={(theme) => patch("appearance", { theme })}
            />
          }
        />
        <SettingsRow
          label="Accent colour"
          description="Used for the main button on each screen, selection and focus."
        >
          <div className="nt-swatches" role="radiogroup" aria-label="Accent colour">
            {ACCENT_PRESETS.map((preset) => (
              <Swatch
                key={preset}
                color={`var(--preset-${preset})`}
                label={ACCENT_LABELS[preset]}
                selected={a.accent === preset}
                onSelect={() => patch("appearance", { accent: preset })}
              />
            ))}
          </div>
        </SettingsRow>
        <SettingsRow
          id="transparency-label"
          label="Frosted controls"
          description="Menus, toasts and floating controls blur what is behind them. Turn off for solid surfaces."
          control={
            <Switch
              aria-labelledby="transparency-label"
              checked={a.transparency}
              onCheckedChange={(transparency) => patch("appearance", { transparency })}
            />
          }
        />
      </SettingsGroup>
      <SettingsGroup title="Notes" footer="Headings stay in Sans.">
        <SettingsRow
          label="Note font"
          description={
            <span className="nt-font-sample" data-font={a.note_font}>
              Ship the shorter onboarding before the pricing change.
            </span>
          }
          control={
            <SegmentedControl
              label="Note font"
              options={FONTS}
              value={a.note_font}
              onChange={(note_font) => patch("appearance", { note_font })}
            />
          }
        />
      </SettingsGroup>
      <SettingsGroup
        title="Home"
        footer="Verbal tics, Overheard, Talk share, Pace, Longest silence, Overtime, Slowest decision and Monologue count your meetings from the last week. The rest read your written notes and Margin's edits. Until a kept face has something to show, Home shows another one; the doodle shows only when nothing else can."
      >
        <SettingsRow
          label="When no note is open"
          description="Keep one, or let Home show a different one each time. Show something else at the bottom of Home still switches for a moment."
          control={
            <SelectMenu
              label="When no note is open"
              value={a.home_face}
              options={HOME_FACES}
              onChange={(home_face) => patch("appearance", { home_face })}
            />
          }
        >
          <HomeFacePreview kept={a.home_face} />
        </SettingsRow>
      </SettingsGroup>
    </SettingsPage>
  );
}
