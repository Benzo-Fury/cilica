/**
 * Cilica Setup's window on Windows (`?window=installer`, apps/setup): install, update or remove
 * Cilica in three screens over the setup ridgeline. The first screen asks, the second shows each
 * step as Setup reports it, the third hands over to Cilica. The sun rises as Cilica installs and
 * sets as it is removed.
 *
 * Setup is its own program: this window talks only to `setupApi`, never to Cilica's `api`, and
 * has no Cilica settings to read, so it follows the Windows theme.
 */
import { ArrowRight } from "lucide-react";
import { type ReactNode, useEffect, useLayoutEffect, useState } from "react";
import { ErrorBoundary } from "@/app/ErrorBoundary";
import { os } from "@/app/platform";
import { useWindowControls } from "@/features/shell/useWindowControls";
import { isTauri } from "@/ipc/client";
import {
  type InstallOptions,
  type SetupInfo,
  setupApi,
  type UninstallOptions,
} from "@/ipc/installer/client";
import {
  Button,
  Icon,
  InstallerActions,
  InstallerAppIcon,
  InstallerBand,
  InstallerFrame,
  InstallerHint,
  InstallerLink,
  InstallerOption,
  InstallerOptions,
  InstallerPath,
  InstallerProblem,
  InstallerProgress,
  InstallerScreen,
  InstallerSpacer,
  SceneArt,
  Switch,
  WindowControls,
} from "@/ui";
import { STEP_WORDS } from "./copy";
import { isBusy, type JobState, sunHeight } from "./flow";
import { useJob, useSetupInfo } from "./hooks";

const OPTIONS_ID = "installer-options";

export function InstallerWindow() {
  return (
    <ErrorBoundary>
      <Installer />
    </ErrorBoundary>
  );
}

function Installer() {
  const { info, error } = useSetupInfo();
  const job = useJob();
  const [ready, setReady] = useState(false);
  const controls = useWindowControls();

  // No Cilica settings here: the theme follows Windows, and glass stays glass inside Setup.
  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.os = os;
    if (!isTauri) root.dataset.material = "opaque";
  }, []);

  // Start the art's rise once the first frame is on screen (a timer: the window starts hidden).
  useEffect(() => {
    const timer = setTimeout(() => setReady(true), 30);
    return () => clearTimeout(timer);
  }, []);

  const mode = info?.mode ?? "install";
  const busy = isBusy(job.state);
  const quit = () => {
    void setupApi.quit().catch(() => undefined);
  };

  return (
    <InstallerFrame
      ready={ready}
      art={<SceneArt sun={sunHeight(mode, job.state)} />}
      band={
        <InstallerBand
          icon={<InstallerAppIcon src="/favicon.svg" />}
          title={mode === "install" ? "Cilica Setup" : "Uninstall Cilica"}
          end={
            <WindowControls onMinimize={controls.onMinimize} onClose={quit} closeDisabled={busy} />
          }
        />
      }
    >
      {error ? (
        <InstallerScreen title="Setup can't start">
          <InstallerProblem>{error.message}</InstallerProblem>
          <InstallerSpacer />
          <InstallerActions>
            <Button variant="secondary" size="lg" autoFocus onClick={quit}>
              Close
            </Button>
          </InstallerActions>
        </InstallerScreen>
      ) : !info ? null : job.state.screen === "start" ? (
        info.mode === "install" ? (
          <InstallStart info={info} onInstall={job.install} />
        ) : (
          <UninstallStart info={info} onUninstall={job.uninstall} onKeep={quit} />
        )
      ) : job.state.screen === "working" ? (
        <Working info={info} state={job.state} onRetry={job.retry} onClose={quit} />
      ) : (
        <Done info={info} deletedData={job.state.steps.includes("delete_data")} onClose={quit} />
      )}
    </InstallerFrame>
  );
}

// ───────── Install ─────────

function InstallStart({
  info,
  onInstall,
}: {
  info: SetupInfo;
  onInstall: (options: InstallOptions) => void;
}) {
  const [open, setOpen] = useState(false);
  const [dir, setDir] = useState(info.default_dir);
  const [desktop, setDesktop] = useState(false);
  const [signIn, setSignIn] = useState(false);
  const [folderError, setFolderError] = useState<string | null>(null);
  const existing = info.existing;

  const change = () => {
    setFolderError(null);
    setupApi
      .chooseFolder(dir)
      .then((picked) => picked && setDir(picked))
      .catch((e: { message?: string }) =>
        setFolderError(e.message ?? "That folder can't be used."),
      );
  };

  if (existing) {
    const same = existing.version === info.version;
    return (
      <InstallerScreen
        title={
          <>
            {same ? "Reinstall" : "Update"} <em>Cilica</em>
          </>
        }
        lead={
          same
            ? `Cilica ${info.version} is already installed. Reinstalling puts back any missing files. Your notes and settings stay as they are.`
            : `Cilica ${existing.version} is installed. Setup will update it to ${info.version}. Your notes and settings stay as they are.`
        }
      >
        <InstallerHint>
          In <InstallerPath>{existing.dir}</InstallerPath>
        </InstallerHint>
        {info.cilica_running && (
          <InstallerHint>Cilica is open. Setup will close it first.</InstallerHint>
        )}
        <InstallerSpacer />
        <InstallerActions>
          <Button
            variant="primary"
            size="lg"
            autoFocus
            onClick={() =>
              onInstall({ dir: existing.dir, desktop_shortcut: false, open_at_sign_in: false })
            }
          >
            {same ? "Reinstall" : "Update"}
            <Icon icon={ArrowRight} size="sm" motion="arrow" />
          </Button>
        </InstallerActions>
      </InstallerScreen>
    );
  }

  return (
    <InstallerScreen
      title={
        <>
          Install <em>Cilica</em>
        </>
      }
      lead="A notes app with an AI assistant and a meeting summariser. Your notes stay on this PC."
    >
      <InstallerOptions id={OPTIONS_ID} open={open} label="Install options">
        <InstallerOption
          id="installer-folder"
          label="Folder"
          detail={folderError ?? <InstallerPath>{dir}</InstallerPath>}
          control={
            <Button variant="ghost" size="sm" onClick={change} aria-describedby="installer-folder">
              Change
            </Button>
          }
        />
        <InstallerOption
          id="installer-desktop"
          label="Desktop shortcut"
          control={
            <Switch
              checked={desktop}
              onCheckedChange={setDesktop}
              aria-labelledby="installer-desktop"
            />
          }
        />
        <InstallerOption
          id="installer-sign-in"
          label="Open Cilica when you sign in"
          control={
            <Switch
              checked={signIn}
              onCheckedChange={setSignIn}
              aria-labelledby="installer-sign-in"
            />
          }
        />
      </InstallerOptions>
      <InstallerActions>
        <Button
          variant="primary"
          size="lg"
          autoFocus
          onClick={() => onInstall({ dir, desktop_shortcut: desktop, open_at_sign_in: signIn })}
        >
          Install
          <Icon icon={ArrowRight} size="sm" motion="arrow" />
        </Button>
        <InstallerHint>
          For you only, no administrator needed.{" "}
          <InstallerLink expanded={open} controls={OPTIONS_ID} onClick={() => setOpen(!open)}>
            {open ? "Hide options" : "Options"}
          </InstallerLink>
        </InstallerHint>
      </InstallerActions>
      <InstallerSpacer />
      <InstallerHint>
        Takes {info.size_mb} MB. The speech model downloads later, in Cilica's own setup.
      </InstallerHint>
    </InstallerScreen>
  );
}

// ───────── Uninstall ─────────

function UninstallStart({
  info,
  onUninstall,
  onKeep,
}: {
  info: SetupInfo;
  onUninstall: (options: UninstallOptions) => void;
  onKeep: () => void;
}) {
  const [deleteData, setDeleteData] = useState(false);

  if (!info.existing) {
    return (
      <InstallerScreen
        title={
          <>
            <em>Cilica</em> isn't installed
          </>
        }
        lead="There's nothing to remove from this PC."
      >
        <InstallerSpacer />
        <InstallerActions>
          <Button variant="secondary" size="lg" autoFocus onClick={onKeep}>
            Close
          </Button>
        </InstallerActions>
      </InstallerScreen>
    );
  }

  return (
    <InstallerScreen
      title={
        <>
          Remove <em>Cilica</em>?
        </>
      }
      lead="Cilica will be removed from this PC. Your notes stay unless you choose otherwise."
    >
      <InstallerOptions label="Remove options">
        <InstallerOption
          id="installer-delete"
          label="Also delete notes, recordings and settings"
          detail={
            deleteData ? (
              "This can't be undone. Saved API keys are removed too."
            ) : (
              <>
                They're kept in <InstallerPath>{info.data_dir}</InstallerPath>
              </>
            )
          }
          control={
            <Switch
              checked={deleteData}
              onCheckedChange={setDeleteData}
              aria-labelledby="installer-delete"
              data-tone="danger"
            />
          }
        />
      </InstallerOptions>
      {info.cilica_running && (
        <InstallerHint>Cilica is open. Setup will close it first.</InstallerHint>
      )}
      <InstallerSpacer />
      <InstallerActions>
        <Button
          variant={deleteData ? "danger" : "primary"}
          size="lg"
          onClick={() => onUninstall({ delete_data: deleteData })}
        >
          {deleteData ? "Remove Cilica and notes" : "Remove Cilica"}
        </Button>
        <Button variant="ghost" size="lg" autoFocus onClick={onKeep}>
          Keep Cilica
        </Button>
      </InstallerActions>
    </InstallerScreen>
  );
}

// ───────── Progress ─────────

function Working({
  info,
  state,
  onRetry,
  onClose,
}: {
  info: SetupInfo;
  state: JobState;
  onRetry: () => void;
  onClose: () => void;
}) {
  const verb = info.mode === "uninstall" ? "Removing" : info.existing ? "Updating" : "Installing";
  const stopped =
    info.mode === "uninstall"
      ? "wasn't removed"
      : info.existing
        ? "wasn't updated"
        : "wasn't installed";
  const failed = state.error !== null;
  const status = failed
    ? "Stopped"
    : state.current
      ? `${STEP_WORDS[state.current].doing}…`
      : "Starting…";
  return (
    <InstallerScreen
      title={
        failed ? (
          <>
            <em>Cilica</em> {stopped}
          </>
        ) : (
          <>
            {verb} <em>Cilica</em>
          </>
        )
      }
    >
      {state.error && <InstallerProblem>{state.error.message}</InstallerProblem>}
      <InstallerProgress
        status={status}
        value={state.progress}
        label={info.mode === "uninstall" ? "Uninstall progress" : "Install progress"}
        done={state.done.map((step) => ({ key: step, text: STEP_WORDS[step].done }))}
        failed={failed}
      />
      <InstallerSpacer />
      {failed && (
        <InstallerActions>
          <Button variant="primary" size="lg" autoFocus onClick={onRetry}>
            Try again
          </Button>
          <Button variant="ghost" size="lg" onClick={onClose}>
            Close
          </Button>
        </InstallerActions>
      )}
    </InstallerScreen>
  );
}

// ───────── Done ─────────

function Done({
  info,
  deletedData,
  onClose,
}: {
  info: SetupInfo;
  deletedData: boolean;
  onClose: () => void;
}) {
  const [opening, setOpening] = useState(false);
  const [problem, setProblem] = useState<ReactNode>(null);

  if (info.mode === "uninstall") {
    return (
      <InstallerScreen
        title={
          <>
            <em>Cilica</em> is removed
          </>
        }
        lead={
          deletedData ? (
            "Your notes, recordings, settings and saved keys were deleted."
          ) : (
            <>
              Your notes are still in <InstallerPath>{info.data_dir}</InstallerPath>. Install Cilica
              again to pick up where you left off.
            </>
          )
        }
      >
        <InstallerSpacer />
        <InstallerActions>
          <Button variant="secondary" size="lg" autoFocus onClick={onClose}>
            Close
          </Button>
        </InstallerActions>
      </InstallerScreen>
    );
  }

  const open = () => {
    setOpening(true);
    setProblem(null);
    setupApi.openCilica().catch((e: { message?: string }) => {
      setOpening(false);
      setProblem(e.message ?? "Cilica didn't open. Open it from the Start menu.");
    });
  };

  return (
    <InstallerScreen
      title={
        <>
          <em>Cilica</em> is {info.existing ? "updated" : "installed"}
        </>
      }
      lead={
        info.existing
          ? `Cilica ${info.version} is ready. Your notes and settings are as you left them.`
          : "It's in the Start menu. Open it to set up your microphone, speech model and notes. That takes about 2 minutes."
      }
    >
      {problem && <InstallerProblem>{problem}</InstallerProblem>}
      <InstallerSpacer />
      <InstallerActions>
        <Button variant="primary" size="lg" autoFocus loading={opening} onClick={open}>
          Open Cilica
          <Icon icon={ArrowRight} size="sm" motion="arrow" />
        </Button>
        <Button variant="ghost" size="lg" onClick={onClose}>
          Close
        </Button>
      </InstallerActions>
    </InstallerScreen>
  );
}
