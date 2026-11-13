/**
 * Recording state and actions: the capsule, sidebar, island and ⌘⇧R all go through these.
 * State comes from `getRecordingState` and stays current through `recording_state_changed`
 * (the event bridge writes it into the query cache).
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { create } from "zustand";
import { useBackendEvent } from "@/app/backendEvents";
import { qk } from "@/app/queryKeys";
import { openNote, openSettings, useRouter } from "@/app/router";
import { useSettings, useUpdateSettings } from "@/features/settings/hooks";
import { api, errorMessage, toAppError } from "@/ipc/client";
import type { AudioLevels, RecordingState, RecordingTrigger, Settings } from "@/ipc/types";
import { cue } from "@/lib/sound";
import { toast } from "@/lib/toast";

export { type CapsuleMode, capsuleMode, isFinishing } from "./model";

const IDLE: RecordingState = {
  phase: "idle",
  note_id: null,
  meeting_app: null,
  started_at: null,
  auto_stop_at: null,
  live_speakers: false,
};

/** Live recording state: fetched once, then kept current by `recording_state_changed`. */
export function useRecordingState(): RecordingState {
  const { data } = useQuery({
    queryKey: qk.recording,
    queryFn: () => api.getRecordingState(),
    staleTime: Number.POSITIVE_INFINITY,
  });
  return data ?? IDLE;
}

export const isLive = (state: RecordingState) =>
  state.phase === "starting" || state.phase === "recording";

// ───────── UI state: the one-time consent notice ─────────

interface RecorderUi {
  /** `before`: asked before the first recording; `after`: it started from the tray or a hotkey. */
  consent:
    | { mode: "before"; trigger: RecordingTrigger; noteId: string | null }
    | { mode: "after" }
    | null;
  /** Set when the UI itself asked to stop, so the stop cue is not played twice. */
  stopCued: boolean;
  setConsent: (consent: RecorderUi["consent"]) => void;
  setStopCued: (value: boolean) => void;
}

export const useRecorderUi = create<RecorderUi>()((set) => ({
  consent: null,
  stopCued: false,
  setConsent: (consent) => set({ consent }),
  setStopCued: (stopCued) => set({ stopCued }),
}));

/** Say why recording did not start, with the one action that fixes it. */
function startFailed(error: unknown) {
  const appError = toAppError(error);
  switch (appError.code) {
    case "unavailable":
      toast.error("Recording did not start", {
        description: appError.message,
        action: { label: "Download model", onClick: () => openSettings("transcription") },
      });
      return;
    case "permission_denied":
      toast.error("Cilica cannot hear the microphone", {
        description: appError.message,
        action: {
          label: "Open Settings",
          onClick: () => void api.openPermissionSettings("microphone").catch(() => undefined),
        },
      });
      return;
    case "not_implemented":
      toast.error("Recording did not start", {
        description:
          "Recording is not available in this build yet. Try again after the next update.",
      });
      return;
    default:
      toast.error("Recording did not start", { description: appError.message });
  }
}

/** Start a recording (into a new meeting note, or into `noteId`), without the consent check. */
export function useStartRecording() {
  const queryClient = useQueryClient();
  return useCallback(
    async (trigger: RecordingTrigger = "manual", noteId: string | null = null) => {
      try {
        const next = await api.startRecording({ note_id: noteId, meeting_app: null, trigger });
        queryClient.setQueryData(qk.recording, next);
        void queryClient.invalidateQueries({ queryKey: qk.notes });
        if (next.note_id) openNote(next.note_id, { panel: "transcript" });
      } catch (error) {
        startFailed(error);
      }
    },
    [queryClient],
  );
}

/** Stop capture. The stop cue plays first so it lands before capture ends. */
export function useStopRecording() {
  const queryClient = useQueryClient();
  const { data: settings } = useSettings();
  return useCallback(async () => {
    cue(settings?.sounds, "stop");
    useRecorderUi.getState().setStopCued(true);
    try {
      queryClient.setQueryData(qk.recording, await api.stopRecording());
    } catch (error) {
      useRecorderUi.getState().setStopCued(false);
      toast.error("Recording did not stop", { description: errorMessage(error) });
    }
  }, [queryClient, settings?.sounds]);
}

/**
 * Start or stop the recording. Starting records into a new meeting note, or into `noteId`
 * (the transcript continues after the existing one). The first start shows the consent notice.
 */
export function useToggleRecording() {
  const queryClient = useQueryClient();
  const start = useStartRecording();
  const stop = useStopRecording();
  return useCallback(
    async (trigger: RecordingTrigger = "manual", noteId: string | null = null) => {
      const state = queryClient.getQueryData<RecordingState>(qk.recording) ?? IDLE;
      if (isLive(state)) return stop();
      if (state.phase !== "idle") return; // finishing the last meeting
      const settings = queryClient.getQueryData<Settings>(qk.settings);
      if (settings && !settings.recording.consent_notice_seen) {
        useRecorderUi.getState().setConsent({ mode: "before", trigger, noteId });
        return;
      }
      return start(trigger, noteId);
    },
    [queryClient, start, stop],
  );
}

export function useKeepRecording() {
  const queryClient = useQueryClient();
  return useCallback(async () => {
    try {
      queryClient.setQueryData(qk.recording, await api.keepRecording());
      toast("Still recording", { description: "Cilica keeps recording until you stop it." });
    } catch (error) {
      toast.error("Recording will still stop", { description: errorMessage(error) });
    }
  }, [queryClient]);
}

/** Answer the consent notice: remember it was seen, then start (or stop) as asked. */
export function useConsentActions() {
  const { patch } = useUpdateSettings();
  const start = useStartRecording();
  const stop = useStopRecording();
  const setConsent = useRecorderUi((s) => s.setConsent);
  return {
    accept: () => {
      const consent = useRecorderUi.getState().consent;
      patch("recording", { consent_notice_seen: true });
      setConsent(null);
      if (consent?.mode === "before") void start(consent.trigger, consent.noteId);
    },
    stopNow: () => {
      patch("recording", { consent_notice_seen: true });
      setConsent(null);
      void stop();
    },
    dismiss: () => setConsent(null),
  };
}

// ───────── Levels and time ─────────

/** Input levels at about 10fps while `active` (the meter needs no more). */
export function useAudioLevels(active: boolean): AudioLevels {
  const [levels, setLevels] = useState<AudioLevels>({ mic: 0, system: 0 });
  const last = useRef(0);
  useBackendEvent("audio_levels", (event) => {
    if (!active) return;
    const now = performance.now();
    if (now - last.current < 90) return;
    last.current = now;
    setLevels(event.levels);
  });
  useEffect(() => {
    if (!active) setLevels({ mic: 0, system: 0 });
  }, [active]);
  return levels;
}

/** A per-bar wobble that changes with each level update, so a steady level still looks alive. */
export function useJitter(levels: AudioLevels, bars = 12): number[] {
  const seed = (levels.mic ?? 0) * 13.7 + (levels.system ?? 0) * 7.3;
  return Array.from({ length: bars }, (_, i) => 0.5 + 0.5 * Math.abs(Math.sin(seed * 9 + i * 1.9)));
}

/** The current time, ticking every second while `active`. */
export function useClock(active: boolean, interval = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const t = setInterval(() => setNow(Date.now()), interval);
    return () => clearInterval(t);
  }, [active, interval]);
  return now;
}

// ───────── App-wide effects ─────────

/**
 * Mounted once in the shell. Plays the start cue once capture has begun and the notes-ready
 * chime when the summary is done, opens a recording's note on its Transcript side when it starts
 * from elsewhere (tray, hotkey, a detected meeting), and shows the consent notice after a first
 * recording that did not start from the UI.
 */
export function useRecordingEffects() {
  const state = useRecordingState();
  const { data: settings } = useSettings();
  const previous = useRef(state.phase);
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  useEffect(() => {
    const from = previous.current;
    const to = state.phase;
    previous.current = to;
    if (from === to) return;
    const sounds = settingsRef.current?.sounds;
    if (to === "recording" && from !== "recording") {
      cue(sounds, "start");
      useRecorderUi.getState().setStopCued(false);
      if (settingsRef.current && !settingsRef.current.recording.consent_notice_seen) {
        if (!useRecorderUi.getState().consent)
          useRecorderUi.getState().setConsent({ mode: "after" });
      }
    }
    if ((to === "starting" || to === "recording") && from === "idle" && state.note_id) {
      const view = useRouter.getState().view;
      if (view.name !== "note" || view.noteId !== state.note_id) {
        openNote(state.note_id, { panel: "transcript" });
      }
    }
    if (from === "recording" && to === "stopping") {
      // Stopped from elsewhere (tray, auto-stop): cue as soon as we hear about it.
      if (!useRecorderUi.getState().stopCued) cue(sounds, "stop");
      useRecorderUi.getState().setStopCued(false);
    }
    if (to === "idle" && from === "summarizing") cue(sounds, "ready");
  }, [state.phase, state.note_id]);
}
