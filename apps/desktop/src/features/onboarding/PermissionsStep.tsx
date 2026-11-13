/**
 * Step 1: the OS permissions recording needs, as buttons that invite and then confirm, plus the
 * choice to record when a meeting starts (Accessibility only matters for that). Only the
 * microphone is needed to go on; you can write notes without recording, so it can be skipped.
 */
import { AppWindow, Mic, Volume2 } from "lucide-react";
import { useState } from "react";
import { useSettings, useUpdateSettings } from "@/features/settings/hooks";
import {
  openPermissionSettings,
  usePermissions,
  useRequestPermission,
} from "@/features/settings/system";
import type { DetectMode, PermissionKind, PermissionStatus } from "@/ipc/types";
import {
  Button,
  SegmentedControl,
  SetupNote,
  SetupPanel,
  SetupPanelLine,
  SetupPermission,
  SetupPermissions,
  SetupStep,
  SetupText,
  Switch,
} from "@/ui";
import { type StepProps, THIS_DEVICE } from "./shared";

const ok = (s: PermissionStatus | undefined) => s === "granted" || s === "not_needed";

const DETECT_MODES = [
  { value: "ask_first", label: "Ask me first" },
  { value: "auto_start", label: "Start on its own" },
] as const satisfies readonly { value: DetectMode; label: string }[];

export function PermissionsStep({ status, skipped, onNext, onReopen }: StepProps) {
  const active = status === "active";
  const { data: permissions } = usePermissions(active);
  const request = useRequestPermission();
  const { data: settings } = useSettings();
  const { patch } = useUpdateSettings();
  // macOS may only confirm system audio once a recording starts; asking is as far as we can go.
  const [asked, setAsked] = useState<ReadonlySet<PermissionKind>>(new Set());

  const mic = permissions?.microphone;
  const sys = permissions?.system_audio;
  const ax = permissions?.accessibility;
  const sysOn = ok(sys) || asked.has("system_audio");
  const next: PermissionKind | null = !ok(mic) ? "microphone" : !sysOn ? "system_audio" : null;

  const ask = (kind: PermissionKind, current: PermissionStatus | undefined) => {
    if (current === "denied") {
      openPermissionSettings(kind);
      return;
    }
    request.mutate(kind, { onSettled: () => setAsked((s) => new Set(s).add(kind)) });
  };

  const recording = settings?.recording;
  const detect = recording?.auto_detect ?? false;
  const mode = recording?.detect_mode ?? "ask_first";
  const others = Math.max(0, (recording?.meeting_apps.length ?? 0) - 3);

  const summary =
    skipped && !ok(mic)
      ? "Skipped. Cilica asks for the microphone when you first record"
      : `${sysOn ? "Microphone and call audio on" : "Microphone on"} · ${
          !detect
            ? "you start recordings yourself"
            : mode === "ask_first"
              ? "asks before recording"
              : "records meetings on its own"
        }`;

  return (
    <SetupStep
      id="permissions"
      status={status}
      title="Let Cilica hear your meetings"
      summary={summary}
      skipped={skipped && !ok(mic)}
      onReopen={onReopen}
      skipLabel="Not now"
      onSkip={() => onNext(!ok(mic))}
      description={`Cilica hears your microphone and the call audio, then turns them into text on ${THIS_DEVICE}.`}
      actions={
        <>
          <Button variant="primary" disabled={!ok(mic)} onClick={() => onNext(false)}>
            Continue
          </Button>
          {!ok(mic) && <SetupNote>Allow the microphone to continue.</SetupNote>}
        </>
      }
    >
      <SetupPermissions>
        <SetupPermission
          icon={Mic}
          title="Let Cilica hear you"
          grantedTitle="Cilica can hear you"
          description={
            mic === "denied"
              ? "Microphone is off in System Settings. Turn it on there."
              : "Microphone. Needed to record."
          }
          grantedDescription="Microphone is on"
          granted={ok(mic)}
          primary={next === "microphone"}
          busy={request.isPending && request.variables === "microphone"}
          onClick={() => ask("microphone", mic)}
        />
        <SetupPermission
          icon={Volume2}
          title="Let Cilica hear the call"
          grantedTitle="Cilica can hear the call"
          description="System audio. Hears the others on Zoom, Meet or Teams. No bot joins."
          grantedDescription={
            ok(sys) ? "System audio is on" : "Your Mac confirms this the first time you record"
          }
          granted={sysOn}
          primary={next === "system_audio"}
          busy={request.isPending && request.variables === "system_audio"}
          onClick={() => ask("system_audio", sys)}
        />
        {ax !== "not_needed" && (
          <SetupPermission
            icon={AppWindow}
            title="Let Cilica spot meetings in your browser"
            grantedTitle="Cilica can spot browser meetings"
            description="Accessibility. Reads window titles to find Meet and Teams."
            grantedDescription="Accessibility is on"
            granted={ok(ax)}
            optional
            busy={request.isPending && request.variables === "accessibility"}
            onClick={() => ask("accessibility", ax)}
          />
        )}
      </SetupPermissions>
      <SetupPanel>
        <SetupPanelLine>
          <SetupText
            id="setup-detect"
            title="Record when a meeting starts"
            text={
              others > 0
                ? `Cilica notices Zoom, Teams, Meet and ${others} other call apps.`
                : "Cilica notices Zoom, Teams and Meet."
            }
          />
          <Switch
            aria-labelledby="setup-detect"
            checked={detect}
            onCheckedChange={(auto_detect) => patch("recording", { auto_detect })}
          />
        </SetupPanelLine>
        {detect && (
          <SegmentedControl
            label="When a meeting starts"
            options={DETECT_MODES}
            value={mode}
            onChange={(detect_mode) => patch("recording", { detect_mode })}
          />
        )}
      </SetupPanel>
    </SetupStep>
  );
}
