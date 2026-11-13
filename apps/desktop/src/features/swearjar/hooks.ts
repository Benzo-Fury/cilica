/**
 * The swear jar's queries, and the watchers that put coins in: live transcript lines, words typed
 * into a note, and coming back to the window after swearing somewhere else.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { useBackendEvent } from "@/app/backendEvents";
import { qk } from "@/app/queryKeys";
import type { OnWordEnd } from "@/features/note/editor/swearWatch";
import { isLive, useRecordingState } from "@/features/recorder/hooks";
import { useSettings } from "@/features/settings/hooks";
import { api, errorMessage } from "@/ipc/client";
import type { Segment } from "@/ipc/types";
import { playSound, soundAllowed } from "@/lib/sound";
import { SwearMatcher } from "@/lib/swears";
import { toast } from "@/lib/toast";
import { dropCoins, light, reconcile, useJar } from "./jar";

/** Gives the transcript time to draw a new line (and scroll to it) before a coin leaves it. */
const RENDER_WAIT_MS = 120;

// ---------- Queries ---------- //

/** This week's jar and the weeks before. */
export function useSwearJar(enabled = true) {
  return useQuery({ queryKey: qk.swearJar, queryFn: () => api.getSwearJar(), enabled });
}

/** The jar's word list, ready to match. Null until it has loaded. */
export function useSwearMatcher(): SwearMatcher | null {
  const { data } = useQuery({
    queryKey: qk.swearWords,
    queryFn: () => api.listSwearWords(),
    staleTime: Number.POSITIVE_INFINITY,
  });
  return useMemo(() => (data ? new SwearMatcher(data) : null), [data]);
}

export function useEmptySwearJar() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => api.emptySwearJar(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: qk.swearJar });
      void queryClient.invalidateQueries({ queryKey: qk.homeGlance });
    },
    onError: (error) =>
      toast.error("The jar was not emptied", { description: errorMessage(error) }),
  });
}

// ---------- Helpers ---------- //

/** Cilica is in front: the window is showing and has focus. */
const inFront = () => document.visibilityState === "visible" && document.hasFocus();

/** Where an element sits on screen, or null when it is scrolled out of sight. */
function visibleRect(el: Element | undefined): DOMRect | null {
  if (!el) return null;
  const r = el.getBoundingClientRect();
  if (r.width === 0 && r.height === 0) return null;
  if (r.bottom < 0 || r.top > window.innerHeight) return null;
  for (let box = el.parentElement; box; box = box.parentElement) {
    if (!/(auto|scroll|hidden)/.test(getComputedStyle(box).overflowY)) continue;
    const b = box.getBoundingClientRect();
    if (r.bottom < b.top || r.top > b.bottom) return null;
  }
  return r;
}

/** The clink as a coin lands: only when switched on, sounds are on, and nothing is recording. */
function useClink(): () => void {
  const { data: settings } = useSettings();
  const recording = useRecordingState();
  const ref = useRef({ settings, live: isLive(recording) });
  ref.current = { settings, live: isLive(recording) };
  return useCallback(() => {
    const { settings: s, live } = ref.current;
    if (s?.swear_jar.clink && soundAllowed(s.sounds, "clink") && !live) playSound("clink");
  }, []);
}

// ---------- Watchers ---------- //

/**
 * Mounted once, in the canvas. Lights up swears in live transcript lines and sends their coins in;
 * keeps the jar's label in step with the backend; and when you come back to Cilica, drops in the
 * coins said while you were away.
 */
export function useSwearJarWatcher() {
  const queryClient = useQueryClient();
  const { data: settings } = useSettings();
  const prefs = settings?.swear_jar;
  const jar = useSwearJar(Boolean(prefs?.enabled));
  const matcher = useSwearMatcher();
  const onLand = useClink();
  const popup = Boolean(prefs?.enabled && prefs.popup);
  const resync = useRef(false);

  // A setting changed what counts: the label follows without coins flying
  const prefsKey = prefs ? JSON.stringify(prefs) : "";
  const lastPrefs = useRef(prefsKey);
  useEffect(() => {
    if (lastPrefs.current === prefsKey) return;
    lastPrefs.current = prefsKey;
    resync.current = true;
    void queryClient.invalidateQueries({ queryKey: qk.swearJar });
  }, [prefsKey, queryClient]);

  // A fresh count while you are looking: line the label up with it
  const count = jar.data?.count;
  useEffect(() => {
    if (count === undefined) return;
    if (useJar.getState().seen !== null && !inFront()) return;
    reconcile(count, { animate: popup && !resync.current, away: false });
    resync.current = false;
  }, [count, popup]);

  // Back from somewhere else: whatever was said meanwhile drops in
  useEffect(() => {
    if (!prefs?.enabled) return;
    const back = () => {
      if (!inFront()) return;
      void jar.refetch().then(({ data }) => {
        if (data) reconcile(data.count, { animate: popup, away: true });
      });
    };
    window.addEventListener("focus", back);
    document.addEventListener("visibilitychange", back);
    return () => {
      window.removeEventListener("focus", back);
      document.removeEventListener("visibilitychange", back);
    };
  }, [prefs?.enabled, popup, jar.refetch]);

  // A finished line with a swear in it: wash the words, then send a coin from each
  useBackendEvent("transcript_segment", ({ segment }) => {
    if (!segment.is_final || !prefs?.enabled || !prefs.transcripts || !matcher) return;
    const hits = matcher.find(segment.text, true);
    if (hits.length === 0 || !inFront()) return;
    light(segment.id);
    setTimeout(() => {
      const marks = document.querySelectorAll(
        `[data-segment-id="${CSS.escape(segment.id)}"] .nt-swear`,
      );
      dropCoins(
        hits.map((_, i) => visibleRect(marks[i])),
        hits.length,
        { onLand },
      );
      void queryClient.invalidateQueries({ queryKey: qk.swearJar });
    }, RENDER_WAIT_MS);
  });
}

/** For the note editor: count a swear as you finish typing it. Stable across renders. */
export function useSwearTyping(noteId: string): OnWordEnd {
  const { data: settings } = useSettings();
  const matcher = useSwearMatcher();
  const onLand = useClink();
  const ref = useRef({ noteId, prefs: settings?.swear_jar, matcher });
  ref.current = { noteId, prefs: settings?.swear_jar, matcher };
  return useCallback(
    (word, rect) => {
      const { noteId: id, prefs, matcher: m } = ref.current;
      if (!m || !prefs?.enabled || !prefs.notes) return false;
      const hit = m.canonical(word);
      if (!hit) return false;
      dropCoins([rect], 1, { onLand });
      void api.addSwearCoin(id, word).catch(() => undefined);
      return true;
    },
    [onLand],
  );
}

/** The swear ranges to wash in a transcript line while its coins are going in. */
export function useSwearMarks(): (
  segment: Segment,
) => { start: number; end: number }[] | undefined {
  const lit = useJar((s) => s.lit);
  const matcher = useSwearMatcher();
  return useCallback(
    (segment) => (lit.has(segment.id) && matcher ? matcher.find(segment.text, true) : undefined),
    [lit, matcher],
  );
}
