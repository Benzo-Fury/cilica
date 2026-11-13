/** Settings › Recording: meeting detection, the meeting apps list, input device, permissions, audio retention. */
import { Plus, RotateCcw, X } from "lucide-react";
import { useState } from "react";
import type { DetectMode } from "@/ipc/types";
import {
  Button,
  Field,
  Icon,
  SegmentedControl,
  SelectMenu,
  SettingsGroup,
  SettingsPage,
  SettingsRow,
  Switch,
} from "@/ui";
import { useSettings, useUpdateSettings } from "../hooks";
import { useInputDevices, usePermissions } from "../system";
import { PermissionRows } from "./PermissionRows";

const DETECT = [
  { value: "ask_first", label: "Ask first" },
  { value: "auto_start", label: "Start recording" },
] as const satisfies readonly { value: DetectMode; label: string }[];

const RETENTION = [
  { value: "7", label: "7 days" },
  { value: "30", label: "30 days" },
  { value: "90", label: "90 days" },
  { value: "0", label: "Always" },
] as const;

export const DEFAULT_MEETING_APPS = [
  "Zoom",
  "Microsoft Teams",
  "Google Meet",
  "Slack",
  "WhatsApp",
  "FaceTime",
  "Webex",
  "Discord",
  "Around",
];

export function RecordingPage() {
  const { data: settings } = useSettings();
  const { patch } = useUpdateSettings();
  const { data: devices = [] } = useInputDevices();
  const { data: permissions } = usePermissions(true);
  const [newApp, setNewApp] = useState("");
  if (!settings) return null;
  const r = settings.recording;
  const retention = RETENTION.some((o) => o.value === String(r.audio_retention_days))
    ? String(r.audio_retention_days)
    : "30";
  const apps = r.meeting_apps;
  const addApp = () => {
    const name = newApp.trim();
    if (!name) return;
    if (!apps.some((a) => a.toLowerCase() === name.toLowerCase()))
      patch("recording", { meeting_apps: [...apps, name] });
    setNewApp("");
  };
  const isDefaultList =
    apps.length === DEFAULT_MEETING_APPS.length &&
    DEFAULT_MEETING_APPS.every((a) => apps.includes(a));

  return (
    <SettingsPage
      title="Recording"
      description="No bot joins the call. Cilica records your microphone and the call audio on this device."
    >
      <SettingsGroup title="Meetings">
        <SettingsRow
          id="auto-detect"
          label="Listen for meetings"
          description="Notice when a meeting app starts using the microphone."
          control={
            <Switch
              aria-labelledby="auto-detect"
              checked={r.auto_detect}
              onCheckedChange={(auto_detect) => patch("recording", { auto_detect })}
            />
          }
        />
        <SettingsRow
          label="When a meeting starts"
          description={
            r.detect_mode === "ask_first"
              ? "A notification asks first, with a Record button."
              : "Recording starts on its own, with a notification to stop it."
          }
          control={
            <SegmentedControl
              label="When a meeting starts"
              options={DETECT}
              value={r.detect_mode}
              onChange={(detect_mode) => patch("recording", { detect_mode })}
            />
          }
        />
        <SettingsRow
          label="Meeting apps"
          description="Apps that count as a meeting when they hold the microphone."
        >
          <div className="nt-chip-row nt-app-chips">
            {apps.map((app) => (
              <span key={app} className="nt-chip nt-app-chip">
                {app}
                <button
                  type="button"
                  className="nt-chip-x"
                  aria-label={`Remove ${app}`}
                  onClick={() =>
                    patch("recording", { meeting_apps: apps.filter((a) => a !== app) })
                  }
                >
                  <Icon icon={X} size="sm" />
                </button>
              </span>
            ))}
          </div>
          <div className="nt-app-add">
            <Field
              size="sm"
              icon={Plus}
              value={newApp}
              placeholder="Add an app, like Riverside"
              aria-label="Add a meeting app"
              onChange={(e) => setNewApp(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addApp();
                }
              }}
            />
            <Button size="sm" variant="secondary" disabled={!newApp.trim()} onClick={addApp}>
              Add
            </Button>
            {!isDefaultList && (
              <Button
                size="sm"
                variant="ghost"
                icon={RotateCcw}
                iconMotion="undo"
                onClick={() => patch("recording", { meeting_apps: [...DEFAULT_MEETING_APPS] })}
              >
                Reset list
              </Button>
            )}
          </div>
        </SettingsRow>
      </SettingsGroup>
      <SettingsGroup title="Microphone">
        <SettingsRow
          label="Input"
          description="The microphone that hears you. Call audio is always captured from this device's output."
          control={
            <SelectMenu
              label="Input"
              value={r.input_device_id ?? "default"}
              options={[
                {
                  value: "default",
                  label: "System default",
                  hint: devices.find((d) => d.is_default)?.name,
                },
                ...devices.map((d) => ({ value: d.id, label: d.name })),
              ]}
              onChange={(v) => patch("recording", { input_device_id: v === "default" ? null : v })}
            />
          }
        />
      </SettingsGroup>
      <SettingsGroup
        title="Permissions"
        footer="Cilica asks the system for each one. You can change them later in System Settings."
      >
        <div className="nt-settings-perms">
          <PermissionRows permissions={permissions} />
        </div>
      </SettingsGroup>
      <SettingsGroup
        title="Audio"
        footer="Transcripts and notes are kept; only the audio files are removed."
      >
        <SettingsRow
          label="Keep recordings for"
          control={
            <SegmentedControl
              label="Keep recordings for"
              options={RETENTION}
              value={retention}
              onChange={(v) => patch("recording", { audio_retention_days: Number(v) })}
            />
          }
        />
        <SettingsRow
          id="consent"
          label="Recording notice"
          description="Show the reminder to tell people you are recording before the next recording."
          control={
            r.consent_notice_seen ? (
              <Button
                size="sm"
                variant="secondary"
                onClick={() => patch("recording", { consent_notice_seen: false })}
              >
                Show again
              </Button>
            ) : (
              <span className="nt-faint t-callout">Shows next time</span>
            )
          }
        />
      </SettingsGroup>
    </SettingsPage>
  );
}
