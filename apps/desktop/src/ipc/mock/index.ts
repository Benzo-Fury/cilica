/**
 * The in-memory backend used outside Tauri (`bun run dev:web`, vitest).
 *
 * It implements every command in bindings.ts against stateful seed data and emits the same
 * backend events the Rust side does. See README-style notes at the top of each module:
 * notes.ts (notes, categories, versions, Trash, search, settings), recording.ts, assistant.ts, tasks.ts.
 */
import type { BackendEvent, Commands } from "@/ipc/types";
import { assistantCommands } from "./assistant";
import { homeCommands } from "./home";
import { importsCommands } from "./imports";
import { linksCommands } from "./links";
import { nearCommands } from "./near";
import { notesCommands, settingsCommands } from "./notes";
import { recordingCommands } from "./recording";
import { createState, type SeedKind } from "./seed";
import type { MockContext } from "./state";
import { swearCommands } from "./swears";
import { tasksCommands } from "./tasks";
import { updatesCommands } from "./updates";
import { clone } from "./util";
import { windowCommands } from "./window";

export interface MockBackend {
  commands: Commands;
  subscribe(handler: (event: BackendEvent) => void): () => void;
  /** Fan an event out to every subscriber (dev and test helper). */
  emit(event: BackendEvent): void;
  /** Stop every pending timer (recording, streaming turns, task runs, downloads). */
  dispose(): void;
  /** Dev helpers: `__marginMock.simulate.meetingDetected("Zoom")`, `.meetingEnded()`. */
  simulate: { meetingDetected(appName?: string): void; meetingEnded(): void };
}

export interface MockOptions {
  /** "demo" (default) loads the sample notes; "empty" is a fresh install. */
  seed?: SeedKind;
  /** Simulated IPC latency per call, in ms. 0 resolves on the next microtask. */
  latencyMs?: number;
}

/** Each command as a plain synchronous function returning the resolved value. */
type Impl = {
  [K in keyof Commands]: (...args: Parameters<Commands[K]>) => Awaited<ReturnType<Commands[K]>>;
};

export function createMockBackend(options: MockOptions = {}): MockBackend {
  const latency = Math.max(0, options.latencyMs ?? 0);
  const subscribers = new Set<(event: BackendEvent) => void>();

  const emit = (event: BackendEvent) => {
    for (const handler of [...subscribers]) handler(clone(event));
  };
  const ctx: MockContext = { state: createState(options.seed ?? "demo"), emit };

  const { dispose: disposeRecording, simulate, ...recording } = recordingCommands(ctx);
  const { dispose: disposeAssistant, ...assistant } = assistantCommands(ctx);
  const { dispose: disposeTasks, ...tasks } = tasksCommands(ctx);
  const { dispose: disposeImports, ...imports } = importsCommands(ctx);
  const impl: Impl = {
    ...notesCommands(ctx),
    ...linksCommands(ctx),
    ...homeCommands(ctx),
    ...nearCommands(ctx),
    ...settingsCommands(ctx),
    ...swearCommands(ctx),
    ...recording,
    ...assistant,
    ...tasks,
    ...imports,
    ...windowCommands(),
    ...updatesCommands(),
  };

  const wrap =
    (fn: (...args: unknown[]) => unknown) =>
    (...args: unknown[]): Promise<unknown> => {
      const run = () => clone(fn(...args));
      if (latency === 0) return Promise.resolve().then(run);
      return new Promise((resolve, reject) => {
        setTimeout(() => {
          try {
            resolve(run());
          } catch (error) {
            reject(error);
          }
        }, latency);
      });
    };

  const commands = Object.fromEntries(
    Object.entries(impl).map(([name, fn]) => [name, wrap(fn as (...args: unknown[]) => unknown)]),
  ) as unknown as Commands;

  return {
    commands,
    subscribe(handler) {
      subscribers.add(handler);
      return () => {
        subscribers.delete(handler);
      };
    },
    emit,
    simulate,
    dispose() {
      disposeRecording();
      disposeAssistant();
      disposeTasks();
      disposeImports();
    },
  };
}

export type { SeedKind };
