/**
 * The one way features reach the backend.
 *
 * Inside Tauri, `api` forwards to the generated bindings. In a plain browser (`bun run dev:web`)
 * and in vitest it forwards to an in-memory mock with seed data, so the whole UI can be built
 * and previewed without the Rust side. Feature hooks import `api`; UI components never do.
 */
import { commands, events } from "./bindings";
import type { MockBackend } from "./mock";
import type { AppError, BackendEvent, Commands, ErrorCode } from "./types";

/** True inside the Tauri webview. */
export const isTauri: boolean = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Handler = (event: BackendEvent) => void;

let mockPromise: Promise<MockBackend> | null = null;

/** The in-memory backend used outside Tauri. Created once, on first use. */
export function getMockBackend(): Promise<MockBackend> {
  if (!mockPromise) {
    mockPromise = import("./mock").then(({ createMockBackend }) => {
      const params =
        typeof window !== "undefined" ? new URLSearchParams(window.location.search) : null;
      const isTest = import.meta.env.MODE === "test";
      const mock = createMockBackend({
        seed: params?.has("fresh") ? "empty" : "demo",
        latencyMs: isTest ? 0 : 40,
      });
      // Dev aid: drive the mock from the console, e.g. `__marginMock.emit({ type: "toast", … })`.
      if (import.meta.env.DEV && typeof window !== "undefined") {
        (window as unknown as { __marginMock?: MockBackend }).__marginMock = mock;
      }
      return mock;
    });
  }
  return mockPromise;
}

const KNOWN_CODES: ReadonlySet<string> = new Set<ErrorCode>([
  "not_found",
  "invalid",
  "conflict",
  "permission_denied",
  "unavailable",
  "upstream",
  "cancelled",
  "not_implemented",
  "internal",
]);

/** Turn anything a command rejected with into an `AppError`. */
export function toAppError(error: unknown): AppError {
  if (error && typeof error === "object" && "message" in error) {
    const { code, message } = error as { code?: unknown; message?: unknown };
    return {
      code: typeof code === "string" && KNOWN_CODES.has(code) ? (code as ErrorCode) : "internal",
      message: typeof message === "string" && message ? message : "Something went wrong.",
    };
  }
  if (typeof error === "string" && error) return { code: "internal", message: error };
  return { code: "internal", message: "Something went wrong. Try again." };
}

/** The message to show people for a failed command. */
export function errorMessage(error: unknown): string {
  return toAppError(error).message;
}

function wrap<K extends keyof Commands>(name: K): Commands[K] {
  const fn = async (...args: unknown[]) => {
    try {
      if (isTauri) {
        return await (commands[name] as (...a: unknown[]) => Promise<unknown>)(...args);
      }
      const mock = await getMockBackend();
      return await (mock.commands[name] as (...a: unknown[]) => Promise<unknown>)(...args);
    } catch (error) {
      throw toAppError(error);
    }
  };
  return fn as unknown as Commands[K];
}

/** Every backend command, typed exactly like the generated `commands`. Rejects with `AppError`. */
export const api: Commands = Object.fromEntries(
  (Object.keys(commands) as (keyof Commands)[]).map((name) => [name, wrap(name)]),
) as Commands;

/**
 * Subscribe to the single backend event stream. Returns an unsubscribe function.
 * The app listens once (see `app/backendEvents.ts`) and fans events out; features rarely need this.
 */
export function onBackendEvent(handler: Handler): () => void {
  let disposed = false;
  let unlisten: (() => void) | null = null;
  const attach = isTauri
    ? events.backendEvent.listen((event) => handler(event.payload as BackendEvent))
    : getMockBackend().then((mock) => mock.subscribe(handler));
  void attach.then((off) => {
    if (disposed) off();
    else unlisten = off;
  });
  return () => {
    disposed = true;
    unlisten?.();
  };
}
