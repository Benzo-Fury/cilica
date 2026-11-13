/** Settings › Sounds (sound-and-haptics.md): a master switch, one per moment, the notes-ready choice, previews. */
import { Play } from "lucide-react";
import type { ReadySound } from "@/ipc/types";
import { playSound, type SoundKind } from "@/lib/sound";
import {
  IconButton,
  SegmentedControl,
  SettingsGroup,
  SettingsPage,
  SettingsRow,
  Switch,
} from "@/ui";
import { useSettings, useUpdateSettings } from "../hooks";

const READY_SOUNDS = [
  { value: "chime", label: "Chime" },
  { value: "bloom", label: "Bloom" },
  { value: "ready", label: "Ready" },
  { value: "glass", label: "Glass" },
] as const satisfies readonly { value: ReadySound; label: string }[];

function Preview({ kind, ready, label }: { kind: SoundKind; ready?: ReadySound; label: string }) {
  return (
    <IconButton
      icon={Play}
      label={label}
      tooltipSide="top"
      onClick={() => playSound(kind, ready)}
    />
  );
}

export function SoundsPage() {
  const { data: settings } = useSettings();
  const { patch } = useUpdateSettings();
  if (!settings) return null;
  const s = settings.sounds;
  return (
    <SettingsPage
      title="Sounds"
      description="Short cues for moments that matter. Everything else is silent."
    >
      <SettingsGroup>
        <SettingsRow
          id="sounds-on"
          label="Play sounds"
          control={
            <Switch
              aria-labelledby="sounds-on"
              checked={s.enabled}
              onCheckedChange={(enabled) => patch("sounds", { enabled })}
            />
          }
        />
      </SettingsGroup>
      <SettingsGroup
        title="Moments"
        footer="Start and stop play after capture begins and before it ends, so they stay out of the recording."
      >
        <SettingsRow
          id="sound-rec"
          label="Recording started and stopped"
          description="Two soft tones that confirm capture began, even when Cilica is behind your meeting app."
          control={
            <>
              <Preview kind="start" label="Play the start sound" />
              <Switch
                aria-labelledby="sound-rec"
                disabled={!s.enabled}
                checked={s.recording_start_stop}
                onCheckedChange={(recording_start_stop) =>
                  patch("sounds", { recording_start_stop })
                }
              />
            </>
          }
        />
        <SettingsRow
          id="sound-ready"
          label="Notes ready"
          description="A warm chime when the meeting summary is written."
          control={
            <>
              <Preview
                kind="ready"
                ready={s.notes_ready_sound}
                label="Play the notes-ready sound"
              />
              <Switch
                aria-labelledby="sound-ready"
                disabled={!s.enabled}
                checked={s.notes_ready}
                onCheckedChange={(notes_ready) => patch("sounds", { notes_ready })}
              />
            </>
          }
        />
        <SettingsRow
          label="Notes ready sound"
          control={
            <SegmentedControl
              label="Notes ready sound"
              options={READY_SOUNDS}
              value={s.notes_ready_sound}
              onChange={(notes_ready_sound) => {
                patch("sounds", { notes_ready_sound });
                playSound("ready", notes_ready_sound);
              }}
            />
          }
        />
        <SettingsRow
          id="sound-ui"
          label="Interface sounds"
          description="Soft ticks for undo, copy and finished tasks."
          control={
            <>
              <Preview kind="tick" label="Play the tick" />
              <Switch
                aria-labelledby="sound-ui"
                disabled={!s.enabled}
                checked={s.interface_sounds}
                onCheckedChange={(interface_sounds) => patch("sounds", { interface_sounds })}
              />
            </>
          }
        />
      </SettingsGroup>
    </SettingsPage>
  );
}
