/**
 * Settings that talk to the OS and the keychain: permissions, input devices, speech models and
 * their downloads, provider keys and the Claude Code bridge.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { create } from "zustand";
import { useBackendEvent } from "@/app/backendEvents";
import { qk } from "@/app/queryKeys";
import { api, errorMessage } from "@/ipc/client";
import type {
  LocalLlmModel,
  ModelDownloadProgress,
  PermissionKind,
  Permissions,
  ProviderKind,
  SttEngine,
  SttModel,
} from "@/ipc/types";
import { haptic } from "@/lib/haptics";
import { toast } from "@/lib/toast";

// ───────── Permissions ─────────

/** OS permissions. Polled while a page that shows them is open, so a grant in System Settings lands. */
export function usePermissions(poll = false) {
  return useQuery({
    queryKey: qk.permissions,
    queryFn: () => api.getPermissions(),
    refetchOnWindowFocus: true,
    refetchInterval: poll ? 3000 : false,
  });
}

export function useRequestPermission() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (kind: PermissionKind) => api.requestPermission(kind),
    onSuccess: (status, kind) => {
      queryClient.setQueryData<Permissions>(qk.permissions, (p) =>
        p ? { ...p, [kind]: status } : p,
      );
      if (status === "granted") haptic("generic");
      void queryClient.invalidateQueries({ queryKey: qk.permissions });
    },
    onError: (error) =>
      toast.error("Margin could not ask for that", { description: errorMessage(error) }),
  });
}

export function openPermissionSettings(kind: PermissionKind) {
  api
    .openPermissionSettings(kind)
    .catch((error: unknown) =>
      toast.error("System Settings did not open", { description: errorMessage(error) }),
    );
}

export function useInputDevices() {
  return useQuery({ queryKey: qk.inputDevices, queryFn: () => api.listInputDevices() });
}

// ───────── Speech models ─────────

export function useSttModels() {
  return useQuery({ queryKey: qk.sttModels, queryFn: () => api.listSttModels() });
}

export interface Download {
  downloaded: number | null;
  total: number | null;
  error: string | null;
}

/** Downloads in flight, fed by `model_download` events (they keep going in the background). */
export const useDownloads = create<{
  downloads: Record<string, Download>;
  set: (modelId: string, download: Download | null) => void;
}>()((set) => ({
  downloads: {},
  set: (modelId, download) =>
    set((s) => {
      const downloads = { ...s.downloads };
      if (download) downloads[modelId] = download;
      else delete downloads[modelId];
      return { downloads };
    }),
}));

/** Downloads the person cancelled: their closing `error` event is expected, not a failure. */
const cancelled = new Set<string>();

/** The diarization models download under this id the first time speakers are told apart. */
export const SPEAKER_MODELS = "speaker-models";

/**
 * Mounted once: tracks progress and announces a finished or failed download. A cancelled
 * download keeps its partial file, so downloading again resumes.
 */
export function useModelDownloadEvents() {
  const queryClient = useQueryClient();
  useBackendEvent("model_download", ({ progress }) => {
    const store = useDownloads.getState();
    if (progress.model_id.startsWith(OLLAMA_PREFIX)) {
      onLocalModelProgress(queryClient, progress);
      return;
    }
    const speakerModels = progress.model_id === SPEAKER_MODELS;
    const name = speakerModels
      ? "The speaker models"
      : (queryClient.getQueryData<SttModel[]>(qk.sttModels)?.find((m) => m.id === progress.model_id)
          ?.name ?? "The speech model");
    if (progress.done) {
      store.set(progress.model_id, null);
      cancelled.delete(progress.model_id);
      if (speakerModels) return;
      queryClient.setQueryData<SttModel[]>(qk.sttModels, (list) =>
        list?.map((m) => (m.id === progress.model_id ? { ...m, downloaded: true } : m)),
      );
      void queryClient.invalidateQueries({ queryKey: qk.sttModels });
      toast.success(`${name} is ready`, { description: "Meetings are transcribed on this Mac." });
      return;
    }
    if (progress.error) {
      store.set(progress.model_id, null);
      const wasCancelled = cancelled.delete(progress.model_id) || /cancel/i.test(progress.error);
      if (!wasCancelled) {
        toast.error(`${name} did not download`, {
          description: `${progress.error} Check your connection, then try again in Settings › Transcription.`,
        });
      }
      return;
    }
    store.set(progress.model_id, {
      downloaded: progress.downloaded_bytes,
      total: progress.total_bytes,
      error: null,
    });
  });
}

export function useModelActions() {
  const queryClient = useQueryClient();
  const download = useMutation({
    mutationFn: (modelId: string) => api.downloadSttModel(modelId),
    onMutate: (modelId) =>
      useDownloads.getState().set(modelId, { downloaded: 0, total: null, error: null }),
    onError: (error, modelId) => {
      useDownloads.getState().set(modelId, null);
      toast.error("The download did not start", { description: errorMessage(error) });
    },
  });
  const cancel = useMutation({
    mutationFn: (modelId: string) => api.cancelSttModelDownload(modelId),
    onMutate: (modelId) => {
      cancelled.add(modelId);
    },
    onSuccess: (_r, modelId) => useDownloads.getState().set(modelId, null),
    onError: (_e, modelId) => {
      cancelled.delete(modelId);
    },
  });
  const remove = useMutation({
    mutationFn: (modelId: string) => api.deleteSttModel(modelId),
    onSuccess: (_r, modelId) => {
      queryClient.setQueryData<SttModel[]>(qk.sttModels, (list) =>
        list?.map((m) => (m.id === modelId ? { ...m, downloaded: false } : m)),
      );
      toast("Deleted the speech model");
    },
    onError: (error) =>
      toast.error("The model was not deleted", { description: errorMessage(error) }),
  });
  /** Download a model (if needed), then switch to it once it is ready. */
  const downloadAndUse = useMutation({
    mutationFn: (modelId: string) => api.downloadAndUseSttModel(modelId),
    onMutate: (modelId) => {
      const model = queryClient
        .getQueryData<SttModel[]>(qk.sttModels)
        ?.find((m) => m.id === modelId);
      if (!model?.downloaded) {
        useDownloads.getState().set(modelId, { downloaded: 0, total: null, error: null });
      }
    },
    onError: (error, modelId) => {
      useDownloads.getState().set(modelId, null);
      toast.error("The download did not start", { description: errorMessage(error) });
    },
  });
  return { download, downloadAndUse, cancel, remove };
}

/** What this computer can run (cores, memory, chip) and whether a cloud service suits it better. */
export function useHardwareProfile() {
  return useQuery({
    queryKey: qk.hardwareProfile,
    queryFn: () => api.getHardwareProfile(),
    staleTime: Number.POSITIVE_INFINITY,
  });
}

// ───────── Cloud speech services ─────────

export function useSttProviderStatus(engine: SttEngine) {
  return useQuery({
    queryKey: qk.sttProviderStatus(engine),
    queryFn: () => api.getSttProviderStatus(engine),
    enabled: engine !== "whisper_local",
  });
}

export function useSttProviderKey(engine: SttEngine) {
  const queryClient = useQueryClient();
  const refresh = () =>
    void queryClient.invalidateQueries({ queryKey: qk.sttProviderStatus(engine) });
  const save = useMutation({
    mutationFn: (key: string) => api.setSttProviderKey(engine, key),
    onSuccess: (status) => queryClient.setQueryData(qk.sttProviderStatus(engine), status),
  });
  const clear = useMutation({
    mutationFn: () => api.clearSttProviderKey(engine),
    onSuccess: (status) => {
      queryClient.setQueryData(qk.sttProviderStatus(engine), status);
      refresh();
      toast("Removed the key from your keychain");
    },
    onError: (error) =>
      toast.error("The key was not removed", { description: errorMessage(error) }),
  });
  return { save, clear };
}

/**
 * Stream the sample clip to the transcription engine in Settings. `heard` is what has come back
 * so far while it runs; the mutation resolves with the full result.
 */
export function useTranscriptionTest() {
  const [heard, setHeard] = useState("");
  useBackendEvent("transcription_test_progress", ({ text }) => setHeard(text));
  const run = useMutation({
    mutationFn: () => api.testTranscription(),
    onMutate: () => setHeard(""),
  });
  return { run, heard };
}

// ───────── Local models (Ollama) ─────────

/** Download events for Ollama carry `ollama:<tag>`; installing Ollama itself is `ollama:app`. */
export const OLLAMA_PREFIX = "ollama:";
export const OLLAMA_APP = `${OLLAMA_PREFIX}app`;

export function useOllamaStatus(enabled: boolean) {
  return useQuery({
    queryKey: qk.ollamaStatus,
    queryFn: () => api.getOllamaStatus(),
    enabled,
    refetchOnWindowFocus: true,
  });
}

export function useLocalModels(enabled: boolean) {
  return useQuery({ queryKey: qk.localModels, queryFn: () => api.listLocalModels(), enabled });
}

function refreshLocal(queryClient: ReturnType<typeof useQueryClient>) {
  for (const key of [qk.localModels, qk.ollamaStatus, qk.providerStatus]) {
    void queryClient.invalidateQueries({ queryKey: key });
  }
}

function onLocalModelProgress(
  queryClient: ReturnType<typeof useQueryClient>,
  progress: ModelDownloadProgress,
) {
  const store = useDownloads.getState();
  const id = progress.model_id;
  const isApp = id === OLLAMA_APP;
  const tag = id.slice(OLLAMA_PREFIX.length);
  const name =
    queryClient.getQueryData<LocalLlmModel[]>(qk.localModels)?.find((m) => m.id === tag)?.name ??
    "The local model";
  if (progress.done) {
    store.set(id, null);
    cancelled.delete(id);
    refreshLocal(queryClient);
    if (!isApp) toast.success(`${name} is ready`, { description: "Margin now runs on this Mac." });
    return;
  }
  if (progress.error) {
    refreshLocal(queryClient);
    const wasCancelled = cancelled.delete(id) || /cancel/i.test(progress.error);
    // Shown inline on Settings › Margin, next to the button that tries again.
    store.set(id, wasCancelled ? null : { downloaded: null, total: null, error: progress.error });
    return;
  }
  store.set(id, {
    downloaded: progress.downloaded_bytes,
    total: progress.total_bytes || null,
    error: null,
  });
}

export function useLocalModelActions() {
  const queryClient = useQueryClient();
  const refresh = () => refreshLocal(queryClient);
  const fail = (title: string) => (error: unknown) =>
    toast.error(title, { description: errorMessage(error) });
  const setUp = useMutation({
    mutationFn: () => api.setUpOllama(),
    onMutate: () =>
      useDownloads.getState().set(OLLAMA_APP, { downloaded: 0, total: null, error: null }),
    // The backend ends setup with an `ollama:app` event (done or error), which clears this state.
    onError: (error) =>
      useDownloads
        .getState()
        .set(OLLAMA_APP, { downloaded: null, total: null, error: errorMessage(error) }),
  });
  const pull = useMutation({
    mutationFn: (modelId: string) => api.pullLocalModel(modelId),
    onMutate: (modelId) =>
      useDownloads
        .getState()
        .set(OLLAMA_PREFIX + modelId, { downloaded: 0, total: null, error: null }),
    onError: (error, modelId) =>
      useDownloads.getState().set(OLLAMA_PREFIX + modelId, {
        downloaded: null,
        total: null,
        error: errorMessage(error),
      }),
  });
  const cancel = useMutation({
    mutationFn: (modelId: string) => api.cancelLocalModelPull(modelId),
    onMutate: (modelId) => {
      cancelled.add(OLLAMA_PREFIX + modelId);
    },
    onSuccess: (_r, modelId) => useDownloads.getState().set(OLLAMA_PREFIX + modelId, null),
  });
  const remove = useMutation({
    mutationFn: (modelId: string) => api.deleteLocalModel(modelId),
    onSuccess: () => {
      refresh();
      toast("Deleted the local model");
    },
    onError: fail("The model was not deleted"),
  });
  const choose = useMutation({
    mutationFn: (modelId: string) => api.selectLocalModel(modelId),
    onSuccess: refresh,
    onError: fail("Margin could not switch models"),
  });
  return { setUp, pull, cancel, remove, choose };
}

// ───────── Providers ─────────

export function useProviderKey() {
  const queryClient = useQueryClient();
  const refresh = () => void queryClient.invalidateQueries({ queryKey: qk.providerStatus });
  const save = useMutation({
    mutationFn: ({ kind, key }: { kind: ProviderKind; key: string }) =>
      api.setProviderKey(kind, key),
    onSuccess: refresh,
  });
  const clear = useMutation({
    mutationFn: (kind: ProviderKind) => api.clearProviderKey(kind),
    onSuccess: () => {
      refresh();
      toast("Removed the key from your keychain");
    },
    onError: (error) =>
      toast.error("The key was not removed", { description: errorMessage(error) }),
  });
  return { save, clear };
}

export function useClaudeCode(enabled: boolean) {
  return useQuery({ queryKey: qk.claudeCode, queryFn: () => api.detectClaudeCode(), enabled });
}

export function formatBytes(bytes: number | null): string {
  if (bytes === null) return "";
  const mb = bytes / (1024 * 1024);
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`;
}
