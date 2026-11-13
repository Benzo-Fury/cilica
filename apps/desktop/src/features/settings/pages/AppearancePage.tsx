import { THEMES, themeLook, themeOption } from "@/app/theme/themes";
import { HomeFacePreview } from "@/features/home/HomeFacePreview";
import type { HomeFace, NoteFont } from "@/ipc/types";
import {
  SegmentedControl,
  SelectMenu,
  SettingsGroup,
  SettingsPage,
  SettingsRow,
  Switch,
  ThemeChoice,
  ThemeChoices,
} from "@/ui";
import { useSettings, useUpdateSettings } from "../hooks";

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
    <SettingsPage title="Appearance" description="How Cilica looks on this device.">
      <SettingsGroup>
        <SettingsRow
          label="Theme"
          description="System follows your computer's light and dark setting. Some themes paint a pixel landscape behind the window."
        >
          <ThemeChoices label="Theme">
            {THEMES.map((theme) => (
              <ThemeChoice
                key={theme.value}
                title={theme.label}
                schemes={theme.scheme ? [theme.scheme] : ["light", "dark"]}
                look={themeLook(theme)}
                scene={theme.scene}
                checked={a.theme === theme.value}
                onSelect={() => patch("appearance", { theme: theme.value })}
              />
            ))}
          </ThemeChoices>
        </SettingsRow>
        {themeOption(a.theme).scene && (
          <SettingsRow
            id="scene-headings-label"
            label="Serif headings"
            description="Note titles and headings are set in Newsreader under this theme. Turn off to keep them in Sans."
            control={
              <Switch
                aria-labelledby="scene-headings-label"
                checked={a.scene_headings}
                onCheckedChange={(scene_headings) => patch("appearance", { scene_headings })}
              />
            }
          />
        )}
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
      <SettingsGroup
        title="Notes"
        footer="Headings stay in Sans, unless a theme with a landscape sets them in the serif."
      >
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
        footer="Verbal tics, Overheard, Talk share, Pace, Longest silence, Overtime, Slowest decision and Monologue count your meetings from the last week. The rest read your written notes and Cilica's edits. Until a kept face has something to show, Home shows another one; the doodle shows only when nothing else can."
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
