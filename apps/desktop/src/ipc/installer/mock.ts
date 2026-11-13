/**
 * A pretend Cilica Setup for the browser preview and tests. It plays the same steps as
 * `apps/setup/src/system/simulated.rs`, with the same weights, and writes nothing.
 *
 * Preview: `bun run dev:web`, then http://localhost:1420/?window=installer. Add `&flow=uninstall`
 * for the Remove flow, `&existing` for an update over an installed Cilica, and `&fail=copy` (any
 * step's name) to see that step fail.
 */
import type { AppError } from "../types";
import type { commands, InstallOptions, JobEvent, Mode, Step, UninstallOptions } from "./bindings";

type Commands = typeof commands;

export interface SetupMock {
  commands: Commands;
  subscribe(handler: (event: JobEvent) => void): () => void;
}

export interface SetupMockOptions {
  mode: Mode;
  /** Cilica is already installed: Install becomes an update. */
  existing: boolean;
  /** Step that fails, by name. */
  failAt: string | null;
  /** Pause between moves; 0 in tests. */
  tickMs: number;
}

/** Same as `Step::weight` in apps/setup/src/plan.rs. */
export const STEP_WEIGHT: Record<Step, number> = {
  check: 0.4,
  unpack: 1.2,
  copy: 4,
  shortcut: 0.3,
  sign_in: 0.3,
  register: 0.3,
  remove_files: 3,
  delete_data: 1.2,
  delete_keys: 0.3,
  unregister: 0.3,
};

/** Same as `install_steps` in plan.rs. */
export function installSteps(options: InstallOptions, updating: boolean): Step[] {
  const steps: Step[] = ["check", "unpack", "copy"];
  if (options.desktop_shortcut && !updating) steps.push("shortcut");
  if (options.open_at_sign_in && !updating) steps.push("sign_in");
  steps.push("register");
  return steps;
}

/** Same as `uninstall_steps` in plan.rs. */
export function uninstallSteps(options: UninstallOptions): Step[] {
  return options.delete_data
    ? ["remove_files", "delete_data", "delete_keys", "unregister"]
    : ["remove_files", "unregister"];
}

/** Same as `overall` in plan.rs. */
export function overall(steps: Step[], step: Step, within: number): number {
  const total = steps.reduce((sum, s) => sum + STEP_WEIGHT[s], 0);
  const index = steps.indexOf(step);
  if (index < 0 || total <= 0) return 0;
  const before = steps.slice(0, index).reduce((sum, s) => sum + STEP_WEIGHT[s], 0);
  return Math.min(1, (before + STEP_WEIGHT[step] * Math.min(1, Math.max(0, within))) / total);
}

const VERSION = "0.3.0";
const LOCAL = "C:\\Users\\you\\AppData\\Local\\Cilica";

export function createSetupMock(options: SetupMockOptions): SetupMock {
  const listeners = new Set<(event: JobEvent) => void>();
  const emit = (event: JobEvent) => {
    for (const listener of listeners) listener(event);
  };
  const wait = () => new Promise((resolve) => setTimeout(resolve, options.tickMs));
  const installed = options.existing || options.mode === "uninstall";
  let busy = false;

  const fail = (message: string): AppError => ({ code: "conflict", message });

  async function play(steps: Step[]) {
    for (const step of steps) {
      emit({ type: "step", step, progress: overall(steps, step, 0) });
      const ticks = Math.max(1, Math.round(STEP_WEIGHT[step] * 6));
      for (let tick = 1; tick <= ticks; tick++) {
        await wait();
        emit({ type: "step", step, progress: overall(steps, step, tick / ticks) });
      }
      if (options.failAt === step) {
        emit({
          type: "failed",
          error: {
            code: "internal",
            message: "This is a pretend failure, set by ?fail in the address. Try again.",
          },
        });
        busy = false;
        return;
      }
      emit({ type: "step_done", step, progress: overall(steps, step, 1) });
    }
    emit({ type: "finished" });
    busy = false;
  }

  function start(steps: Step[]): Step[] {
    if (busy) throw fail("Setup is already working. Wait for it to finish.");
    busy = true;
    void play(steps);
    return steps;
  }

  const commands: Commands = {
    getSetupInfo: async () => ({
      mode: options.mode,
      version: VERSION,
      default_dir: LOCAL,
      existing: installed ? { version: options.existing ? "0.2.4" : VERSION, dir: LOCAL } : null,
      data_dir: "C:\\Users\\you\\AppData\\Roaming\\app.cilica.desktop",
      size_mb: 64,
      cilica_running: false,
    }),
    showWindow: async () => null,
    chooseFolder: async () => "D:\\Apps\\Cilica",
    install: async (install: InstallOptions) => start(installSteps(install, options.existing)),
    uninstall: async (uninstall: UninstallOptions) => start(uninstallSteps(uninstall)),
    openCilica: async () => null,
    quit: async () => {
      if (busy) throw fail("Setup is still working. Wait for it to finish, then close it.");
      return null;
    },
  };

  return {
    commands,
    subscribe(handler) {
      listeners.add(handler);
      return () => listeners.delete(handler);
    },
  };
}
