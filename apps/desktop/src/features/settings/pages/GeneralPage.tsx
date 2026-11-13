import { Keyboard, RefreshCw, RotateCw, Sparkle } from "lucide-react";
import { useOpenSetup } from "@/features/onboarding/hooks";
import { useOverlays } from "@/features/shell/overlays";
import {
  checkForUpdate,
  installUpdate,
  restartToUpdate,
  type UpdateProgress,
  useUpdateState,
} from "@/features/updates/hooks";
import { Button, Progress, SettingsGroup, SettingsPage, SettingsRow, Switch } from "@/ui";
import { useAppInfo, useSettings, useUpdateSettings } from "../hooks";

export function GeneralPage() {
  const { data: settings } = useSettings();
  const { data: info } = useAppInfo();
  const { update } = useUpdateSettings();
  const { phase, update: available, progress } = useUpdateState();
  const openSetup = useOpenSetup();
  if (!settings) return null;
  const autoInstall = settings.updates.auto_install;
  return (
    <SettingsPage title="General">
      <SettingsGroup title="Startup">
        <SettingsRow
          id="launch-label"
          label="Open Cilica when you log in"
          description="Cilica starts in the background so it can listen for meetings."
          control={
            <Switch
              aria-labelledby="launch-label"
              checked={settings.launch_at_login}
              onCheckedChange={(launch_at_login) => update((s) => ({ ...s, launch_at_login }))}
            />
          }
        />
      </SettingsGroup>
      <SettingsGroup title="Writing">
        <SettingsRow
          id="spelling-label"
          label="Check spelling"
          description="Underlines misspelt words as you type. Right-click one for suggestions."
          control={
            <Switch
              aria-labelledby="spelling-label"
              checked={settings.writing.check_spelling}
              onCheckedChange={(check_spelling) =>
                update((s) => ({ ...s, writing: { ...s.writing, check_spelling } }))
              }
            />
          }
        />
      </SettingsGroup>
      <SettingsGroup title="Updates">
        <SettingsRow
          id="auto-update-label"
          label="Install updates automatically"
          description="New versions download in the background and apply when Cilica restarts."
          control={
            <Switch
              aria-labelledby="auto-update-label"
              checked={autoInstall}
              onCheckedChange={(auto_install) =>
                update((s) => ({ ...s, updates: { ...s.updates, auto_install } }))
              }
            />
          }
        />
        <SettingsRow
          label={updateLabel(phase, available?.version)}
          description={
            phase === "installing" && progress
              ? downloadedLabel(progress)
              : info
                ? `You have version ${info.version}.`
                : undefined
          }
          control={
            phase === "ready" ? (
              <Button size="sm" variant="primary" icon={RotateCw} onClick={restartToUpdate}>
                Restart to update
              </Button>
            ) : phase === "available" || phase === "installing" ? (
              <Button
                size="sm"
                variant="primary"
                loading={phase === "installing"}
                onClick={() => void installUpdate()}
              >
                Download
              </Button>
            ) : (
              <Button
                size="sm"
                variant="secondary"
                icon={RefreshCw}
                loading={phase === "checking"}
                onClick={() => void checkForUpdate({ manual: true, autoInstall })}
              >
                Check for updates
              </Button>
            )
          }
        >
          {phase === "installing" && progress && (
            <Progress
              value={progress.total ? progress.downloaded / progress.total : null}
              label={`Downloading Cilica ${available?.version ?? ""}`.trim()}
            />
          )}
        </SettingsRow>
      </SettingsGroup>
      <SettingsGroup title="Help">
        <SettingsRow
          label="Keyboard shortcuts"
          description="Every shortcut on one sheet. You can change them in Keybinds."
          control={
            <Button
              size="sm"
              variant="secondary"
              icon={Keyboard}
              onClick={() => useOverlays.getState().show("cheatsheet")}
            >
              Show shortcuts
            </Button>
          }
        />
        <SettingsRow
          label="Setup"
          description="Go through permissions, models, imports and appearance again. Nothing you set up is lost."
          control={
            <Button
              size="sm"
              variant="secondary"
              icon={Sparkle}
              loading={openSetup.isPending}
              onClick={() => openSetup.mutate()}
            >
              Run setup again
            </Button>
          }
        />
      </SettingsGroup>
      {info && (
        <SettingsGroup title="About">
          <SettingsRow
            label="Version"
            control={<span className="nt-faint nt-mono">{info.version}</span>}
          />
          <SettingsRow
            label="Your data"
            description={<span className="nt-selectable nt-mono">{info.data_dir}</span>}
          />
        </SettingsGroup>
      )}
    </SettingsPage>
  );
}

function updateLabel(phase: string, version: string | undefined): string {
  switch (phase) {
    case "checking":
      return "Checking for updates…";
    case "current":
      return "Cilica is up to date";
    case "available":
      return `Cilica ${version} is available`;
    case "installing":
      return `Downloading Cilica ${version}…`;
    case "ready":
      return `Cilica ${version} is ready`;
    case "error":
      return "The last update check failed";
    default:
      return "Updates";
  }
}

function megabytes(bytes: number): string {
  return (bytes / 1_000_000).toFixed(1);
}

/** "4.2 of 18.4 MB", or just the amount so far when the size is unknown. */
function downloadedLabel({ downloaded, total }: UpdateProgress): string {
  return total
    ? `${megabytes(downloaded)} of ${megabytes(total)} MB`
    : `${megabytes(downloaded)} MB so far`;
}
