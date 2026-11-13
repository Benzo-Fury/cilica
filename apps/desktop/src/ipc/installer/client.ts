/**
 * How the installer window (Cilica Setup, `apps/setup`) reaches its backend. Inside Setup,
 * `setupApi` forwards to the generated `./bindings`; in a browser (`bun run dev:web`, then
 * `?window=installer`) and in vitest it forwards to `./mock`, which plays the same steps.
 *
 * Setup is a separate program from Cilica, so it has its own commands and events; Cilica's
 * `api` is never used in the installer window.
 */
import { isTauri, toAppError } from "../client";
import { commands, events, type JobEvent } from "./bindings";
import type { SetupMock } from "./mock";

export type {
  InstalledCilica,
  InstallOptions,
  JobEvent,
  Mode,
  SetupInfo,
  Step,
  UninstallOptions,
} from "./bindings";

type Commands = typeof commands;

let mockPromise: Promise<SetupMock> | null = null;

/** The simulated Setup used outside Tauri. Created once, on first use. */
export function getSetupMock(): Promise<SetupMock> {
  if (!mockPromise) {
    mockPromise = import("./mock").then(({ createSetupMock }) => {
      const params =
        typeof window !== "undefined" ? new URLSearchParams(window.location.search) : null;
      const isTest = import.meta.env.MODE === "test";
      return createSetupMock({
        mode: params?.get("flow") === "uninstall" ? "uninstall" : "install",
        existing: params?.has("existing") ?? false,
        failAt: params?.get("fail") ?? null,
        tickMs: isTest ? 0 : 110,
      });
    });
  }
  return mockPromise;
}

function wrap<K extends keyof Commands>(name: K): Commands[K] {
  const fn = async (...args: unknown[]) => {
    try {
      if (isTauri) return await (commands[name] as (...a: unknown[]) => Promise<unknown>)(...args);
      const mock = await getSetupMock();
      return await (mock.commands[name] as (...a: unknown[]) => Promise<unknown>)(...args);
    } catch (error) {
      throw toAppError(error);
    }
  };
  return fn as unknown as Commands[K];
}

/** Every Setup command, typed like the generated `commands`. Rejects with `AppError`. */
export const setupApi: Commands = Object.fromEntries(
  (Object.keys(commands) as (keyof Commands)[]).map((name) => [name, wrap(name)]),
) as Commands;

/** Listen to install or uninstall progress. Returns an unsubscribe function. */
export function onJobEvent(handler: (event: JobEvent) => void): () => void {
  let disposed = false;
  let unlisten: (() => void) | null = null;
  const attach = isTauri
    ? events.setupEvent.listen((event) => handler(event.payload))
    : getSetupMock().then((mock) => mock.subscribe(handler));
  void attach.then((off) => {
    if (disposed) off();
    else unlisten = off;
  });
  return () => {
    disposed = true;
    unlisten?.();
  };
}
