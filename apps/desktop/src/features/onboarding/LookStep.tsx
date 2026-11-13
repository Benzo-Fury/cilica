/**
 * Step 5, the last: theme and note typeface. A theme change repaints this window and its art at
 * once. Its button opens Cilica; there is no separate ending page.
 */
import { ArrowRight } from "lucide-react";
import { themeOption } from "@/app/theme/themes";
import { useSettings, useUpdateSettings } from "@/features/settings/hooks";
import type { NoteFont, ThemeMode } from "@/ipc/types";
import { Button, Icon, SegmentedControl, SetupChoice, SetupChoices, SetupStep } from "@/ui";

const THEMES = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
] as const satisfies readonly { value: ThemeMode; label: string }[];

const FONTS = [
  { value: "sans", label: "Sans" },
  { value: "serif", label: "Serif" },
  { value: "mono", label: "Mono" },
] as const satisfies readonly { value: NoteFont; label: string }[];

export function lookSummary(theme: ThemeMode, font: NoteFont): string {
  const t = themeOption(theme).label;
  const f = FONTS.find((x) => x.value === font)?.label ?? "Sans";
  return `${t} theme, ${f} notes`;
}

export function LookStep({
  status,
  onReopen,
  onFinish,
  finishing,
}: {
  status: "done" | "active";
  onReopen: () => void;
  onFinish: () => void;
  finishing: boolean;
}) {
  const { data: settings } = useSettings();
  const { patch } = useUpdateSettings();
  const theme = settings?.appearance.theme ?? "system";
  const font = settings?.appearance.note_font ?? "sans";
  return (
    <SetupStep
      id="look"
      status={status}
      title="Make it yours"
      summary={lookSummary(theme, font)}
      onReopen={onReopen}
      description="Pick a theme and the typeface your notes are written in. Both change later in Settings › Appearance."
      actions={
        <Button variant="primary" size="lg" loading={finishing} onClick={onFinish}>
          Open Cilica
          <Icon icon={ArrowRight} size="sm" motion="arrow" />
        </Button>
      }
    >
      <SegmentedControl
        label="Theme"
        options={THEMES}
        value={theme}
        onChange={(value) => patch("appearance", { theme: value })}
      />
      <SetupChoices label="Note typeface" columns={3}>
        {FONTS.map((f) => (
          <SetupChoice
            key={f.value}
            title={f.label}
            preview="Aa"
            font={f.value}
            description="Notes from Thursday's review"
            checked={font === f.value}
            onSelect={() => patch("appearance", { note_font: f.value })}
          />
        ))}
      </SetupChoices>
    </SetupStep>
  );
}
