import { Keyboard, RefreshCw, RotateCw, Sparkle } from "lucide-react";
import { isJarFace } from "@/features/home/glance";
import { imageTextLine, useImageTextStatus } from "@/features/note/images/imageText";
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
        <SettingsRow
          id="shrink-images-label"
          label="Shrink large images"
          description="Resizes images wider or taller than 2560 px when you add them, so notes take less space. Animated GIFs are kept as they are."
          control={
            <Switch
              aria-labelledby="shrink-images-label"
              checked={settings.writing.shrink_large_images}
              onCheckedChange={(shrink_large_images) =>
                update((s) => ({ ...s, writing: { ...s.writing, shrink_large_images } }))
              }
            />
          }
        />
        <ImageTextRow
          checked={settings.writing.find_text_in_images}
          onChange={(find_text_in_images) =>
            update((s) => ({ ...s, writing: { ...s.writing, find_text_in_images } }))
          }
        />
      </SettingsGroup>
      <SettingsGroup title="Tabs">
        <SettingsRow
          id="tabs-label"
          label="Open notes in tabs"
          description="Keeps several notes open in a bar along the bottom of the window. A note you only look at shares one preview tab; notes you create or write in keep their own. Middle-click a note to open it in a new tab."
          control={
            <Switch
              aria-labelledby="tabs-label"
              checked={settings.tabs}
              onCheckedChange={(tabs) => update((s) => ({ ...s, tabs }))}
            />
          }
        />
      </SettingsGroup>
      <SettingsGroup title="Language">
        <SettingsRow
          id="sfw-label"
          label="Safe for work"
          description="Cilica keeps its own words clean and puts the swear jar away. Your notes and transcripts stay as they are."
          control={
            <Switch
              aria-labelledby="sfw-label"
              checked={settings.safe_for_work}
              onCheckedChange={(safe_for_work) =>
                update((s) => ({
                  ...s,
                  safe_for_work,
                  // A kept jar face has nothing to show once the jar is away
                  appearance:
                    safe_for_work && isJarFace(s.appearance.home_face)
                      ? { ...s.appearance, home_face: "random" }
                      : s.appearance,
                }))
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

/**
 * "Find text in images": macOS reads the words in every note image on this Mac, so search and
 * Cilica find them. Hidden where the system has no text recognition. Shows the work in progress
 * after an import or on first run.
 */
function ImageTextRow({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (on: boolean) => void;
}) {
  const { data: status } = useImageTextStatus();
  if (!status?.supported) return null;
  const working = checked && status.running && status.done < status.total;
  const line = imageTextLine(status);
  return (
    <SettingsRow
      id="image-text-label"
      label="Find text in images"
      description={
        <>
          Reads the words in your images on this Mac, so search and Cilica can find them. Nothing
          leaves your computer.
          {line && (
            <>
              <br />
              <span className="nt-faint">{line}</span>
            </>
          )}
        </>
      }
      control={
        <Switch aria-labelledby="image-text-label" checked={checked} onCheckedChange={onChange} />
      }
    >
      {working && (
        <Progress
          value={status.total ? status.done / status.total : null}
          label={`Finding text in ${status.total} images`}
        />
      )}
    </SettingsRow>
  );
}
