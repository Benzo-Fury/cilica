/**
 * Settings › Import: what Margin finds on this computer, previews, running imports and their history.
 * Progress arrives as `import_progress` events and is kept per run in a small store, so it survives
 * leaving the page and coming back.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { create } from "zustand";
import { useBackendEvent } from "@/app/backendEvents";
import { qk } from "@/app/queryKeys";
import { api, errorMessage, isTauri } from "@/ipc/client";
import type { ImportInput, ImportOptions, ImportProgress, ImportRun } from "@/ipc/types";
import { toast } from "@/lib/toast";
import { providerById } from "./providers";

// ── Detection ────────────────────────────────────────────────────────────

export function useImportDetection(provider: string | null) {
  return useQuery({
    queryKey: qk.importDetect(provider ?? ""),
    queryFn: () => api.importDetect(provider ?? ""),
    enabled: Boolean(provider),
    staleTime: 30_000,
  });
}

/** Apps found on this computer, for the picker's badges. */
export function useAllDetections() {
  return useQuery({
    queryKey: qk.importDetectAll,
    queryFn: () => api.importDetectAll(),
    staleTime: 60_000,
  });
}

export function useImportRuns() {
  return useQuery({ queryKey: qk.importRuns, queryFn: () => api.importRuns() });
}

// ── Keys ─────────────────────────────────────────────────────────────────

export function useImportKey(provider: string) {
  const queryClient = useQueryClient();
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: qk.importDetect(provider) });
    void queryClient.invalidateQueries({ queryKey: qk.importDetectAll });
  };
  const save = useMutation({
    mutationFn: (key: string) => api.importSaveKey(provider, key),
    onSuccess: refresh,
  });
  const clear = useMutation({
    mutationFn: () => api.importClearKey(provider),
    onSuccess: () => {
      refresh();
      toast("Removed the key from your keychain");
    },
    onError: (error) =>
      toast.error("The key was not removed", { description: errorMessage(error) }),
  });
  return { save, clear };
}

// ── Scanning and importing ───────────────────────────────────────────────

export function useImportScan() {
  return useMutation({
    mutationFn: ({ provider, input }: { provider: string; input: ImportInput }) =>
      api.importScan(provider, input),
  });
}

export function useStartImport() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ scanId, options }: { scanId: string; options: ImportOptions }) =>
      api.importStart(scanId, options),
    onSuccess: (run) => {
      useImportProgress.getState().started(run);
      void queryClient.invalidateQueries({ queryKey: qk.importRuns });
    },
    onError: (error) =>
      toast.error("The import did not start", { description: errorMessage(error) }),
  });
}

export function useCancelImport() {
  return useMutation({ mutationFn: (runId: string) => api.importCancel(runId) });
}

/** Move an import's notes to Trash. Callers confirm first. */
export function useUndoImport() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (runId: string) => api.importUndo(runId),
    onSuccess: (moved) => {
      void queryClient.invalidateQueries({ queryKey: qk.importRuns });
      toast(
        moved === 1 ? "Moved 1 imported note to Trash" : `Moved ${moved} imported notes to Trash`,
        {
          description: "Restore them from Trash if you change your mind.",
        },
      );
    },
    onError: (error) =>
      toast.error("The import was not undone", { description: errorMessage(error) }),
  });
}

// ── Progress ─────────────────────────────────────────────────────────────

interface ProgressState {
  /** Latest progress per run. */
  runs: Record<string, ImportProgress>;
  started(run: ImportRun): void;
  update(progress: ImportProgress): void;
  forget(runId: string): void;
}

export const useImportProgress = create<ProgressState>((set) => ({
  runs: {},
  started: (run) =>
    set((s) => ({
      runs: {
        ...s.runs,
        [run.id]: s.runs[run.id] ?? {
          run_id: run.id,
          provider: run.provider,
          phase: "reading",
          done: 0,
          total: 0,
          current: null,
          run: null,
        },
      },
    })),
  update: (progress) =>
    set((s) => {
      const previous = s.runs[progress.run_id];
      // A "waiting" notice carries no counts; keep the last ones.
      const next =
        progress.phase === "waiting" && previous
          ? { ...previous, phase: progress.phase, current: progress.current }
          : progress;
      return { runs: { ...s.runs, [progress.run_id]: next } };
    }),
  forget: (runId) =>
    set((s) => {
      const { [runId]: _gone, ...rest } = s.runs;
      return { runs: rest };
    }),
}));

/** The name people know an app by. */
export function providerName(id: string): string {
  return providerById(id)?.name ?? id;
}

/** Feed `import_progress` events into the store and say when an import finishes. Mounted once by the app. */
export function useImportEvents(onUndo: (run: ImportRun) => void) {
  const queryClient = useQueryClient();
  useBackendEvent("import_progress", (event) => {
    const { progress } = event;
    useImportProgress.getState().update(progress);
    const run = progress.run;
    if (progress.phase !== "done" || !run) return;
    void queryClient.invalidateQueries({ queryKey: qk.importRuns });
    const name = providerName(run.provider);
    if (run.status === "failed" && run.imported === 0) {
      toast.error(`The import from ${name} stopped`, {
        description: run.report[0] ?? "Try again in a moment.",
      });
      return;
    }
    if (run.status === "cancelled" && run.imported === 0) {
      toast(`Stopped importing from ${name}`);
      return;
    }
    const brought = run.imported + run.updated;
    if (brought === 0) {
      toast(`Nothing new from ${name}`, {
        description: run.skipped > 0 ? "Everything was already imported." : undefined,
      });
      return;
    }
    const title =
      run.imported > 0
        ? `Imported ${countNotes(run.imported)} from ${name}`
        : `Updated ${countNotes(run.updated)} from ${name}`;
    const description = [
      run.imported > 0 && run.updated > 0 ? `${run.updated} updated` : null,
      run.failed > 0 ? `${run.failed} could not be read` : null,
    ]
      .filter(Boolean)
      .join(" · ");
    if (run.imported > 0)
      toast.undo(title, { description: description || undefined, onUndo: () => onUndo(run) });
    else toast.success(title, { description: description || undefined });
  });
}

export function countNotes(n: number): string {
  return n === 1 ? "1 note" : `${n} notes`;
}

// ── Files ────────────────────────────────────────────────────────────────

/** Ask for files or a folder with the system dialog. Outside Tauri, a sample path stands in. */
export async function pickPaths(
  kind: "files" | "folder",
  accept: readonly string[],
  title: string,
): Promise<string[] | null> {
  if (!isTauri) {
    const name =
      kind === "folder" ? "~/Documents/Sample Vault" : `~/Downloads/export.${accept[0] ?? "md"}`;
    return [name];
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({
    title,
    directory: kind === "folder",
    multiple: true,
    filters:
      kind === "files" && accept.length > 0
        ? [{ name: "Exports", extensions: [...accept] }]
        : undefined,
  });
  if (!picked) return null;
  return Array.isArray(picked) ? picked : [picked];
}

/** Files dragged onto the window while `enabled`. Returns whether something is over the window. */
export function useFileDrop(enabled: boolean, onDrop: (paths: string[]) => void): boolean {
  const [over, setOver] = useState(false);
  useEffect(() => {
    if (!enabled) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    if (isTauri) {
      void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
        getCurrentWebview()
          .onDragDropEvent((event) => {
            const p = event.payload;
            if (p.type === "enter" || p.type === "over") setOver(true);
            else if (p.type === "leave") setOver(false);
            else if (p.type === "drop") {
              setOver(false);
              if (p.paths.length > 0) onDrop(p.paths);
            }
          })
          .then((off) => {
            if (disposed) off();
            else unlisten = off;
          }),
      );
    } else {
      // In a browser there are no paths; the mock takes the file names instead.
      const enter = (e: DragEvent) => {
        if (e.dataTransfer?.types.includes("Files")) {
          e.preventDefault();
          setOver(true);
        }
      };
      const leave = (e: DragEvent) => {
        if (!e.relatedTarget) setOver(false);
      };
      const drop = (e: DragEvent) => {
        e.preventDefault();
        setOver(false);
        const names = Array.from(e.dataTransfer?.files ?? []).map((f) => `~/Downloads/${f.name}`);
        if (names.length > 0) onDrop(names);
      };
      window.addEventListener("dragover", enter);
      window.addEventListener("dragleave", leave);
      window.addEventListener("drop", drop);
      unlisten = () => {
        window.removeEventListener("dragover", enter);
        window.removeEventListener("dragleave", leave);
        window.removeEventListener("drop", drop);
      };
    }
    return () => {
      disposed = true;
      unlisten?.();
      setOver(false);
    };
  }, [enabled, onDrop]);
  return over;
}
