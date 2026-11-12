/**
 * Debounced body saves. Typing schedules a save 600ms after the last change; blur, switching
 * notes, hiding or closing the window flush at once. The last saved Markdown is remembered so
 * a refetch carrying our own save never resets the editor.
 */
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { useBackendEvent } from "@/app/backendEvents";
import { qk } from "@/app/queryKeys";
import { api, errorMessage } from "@/ipc/client";
import type { Note, NoteSummary } from "@/ipc/types";
import { refsSignature } from "@/lib/links";
import { toast } from "@/lib/toast";
import { normalizeMarkdown } from "./editor/markdown";

const DEBOUNCE_MS = 600;

/** Every mounted autosave, so the app can flush them all (e.g. before quitting). */
const flushers = new Set<() => Promise<void>>();
export function flushAllNoteSaves(): Promise<void> {
  return Promise.all([...flushers].map((f) => f())).then(() => undefined);
}

export interface Autosave {
  schedule: (markdown: string) => void;
  flush: () => Promise<void>;
  /** True while there is an unsaved change. */
  isDirty: () => boolean;
  /** The Markdown the backend is known to hold (normalised). */
  known: () => string;
  /** Record a body that came from the backend (load, restore, stream). */
  setKnown: (markdown: string) => void;
}

export function useAutosave(noteId: string, initialBody: string): Autosave {
  const queryClient = useQueryClient();
  const pending = useRef<string | null>(null);
  const known = useRef(normalizeMarkdown(initialBody));
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inflight = useRef<Promise<void> | null>(null);

  const flush = useCallback(async (): Promise<void> => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    if (inflight.current) await inflight.current;
    const body = pending.current;
    if (body === null) return;
    pending.current = null;
    if (normalizeMarkdown(body) === known.current) return;
    const save = api.updateNoteBody(noteId, body).then(
      (summary: NoteSummary) => {
        // The graph and backlinks only need a refetch when the links themselves changed.
        const linksChanged = refsSignature(known.current) !== refsSignature(body);
        known.current = normalizeMarkdown(body);
        if (linksChanged) void queryClient.invalidateQueries({ queryKey: qk.links });
        queryClient.setQueryData<Note>(qk.note(noteId), (note) =>
          note ? { ...note, ...summary, body_md: body } : note,
        );
        queryClient.setQueryData<NoteSummary[]>(qk.notes, (list) =>
          list?.map((n) => (n.id === summary.id ? summary : n)),
        );
        void queryClient.invalidateQueries({ queryKey: qk.versions(noteId) });
      },
      (error) => {
        // Keep the change so the next edit or flush retries it.
        if (pending.current === null) pending.current = body;
        toast.error("Your last edit was not saved", { description: errorMessage(error) });
      },
    );
    inflight.current = save.finally(() => {
      inflight.current = null;
    });
    await inflight.current;
  }, [noteId, queryClient]);

  const schedule = useCallback(
    (markdown: string) => {
      pending.current = markdown;
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => void flush(), DEBOUNCE_MS);
    },
    [flush],
  );

  // The shell announces a quit and waits a moment: save now.
  useBackendEvent("will_quit", () => void flush());

  useEffect(() => {
    flushers.add(flush);
    const onHide = () => void flush();
    const onVisibility = () => document.visibilityState === "hidden" && void flush();
    window.addEventListener("beforeunload", onHide);
    window.addEventListener("pagehide", onHide);
    window.addEventListener("blur", onHide);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      flushers.delete(flush);
      window.removeEventListener("beforeunload", onHide);
      window.removeEventListener("pagehide", onHide);
      window.removeEventListener("blur", onHide);
      document.removeEventListener("visibilitychange", onVisibility);
      void flush(); // switching notes or closing the view
    };
  }, [flush]);

  return useMemo(
    () => ({
      schedule,
      flush,
      isDirty: () => pending.current !== null || inflight.current !== null,
      known: () => known.current,
      setKnown: (markdown: string) => {
        known.current = normalizeMarkdown(markdown);
      },
    }),
    [schedule, flush],
  );
}
