/**
 * Plays a meeting's recording. Inside Tauri it is an <audio> element on the asset URL; the
 * browser preview's mock hands out a `mock:` source, which plays as a silent clock so seeking,
 * highlighting and speed can all be tried without audio files.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { haptic } from "@/lib/haptics";

export const RATES = [1, 1.25, 1.5, 2, 0.75] as const;

interface Engine {
  play(): Promise<void>;
  pause(): void;
  seek(ms: number): void;
  setRate(rate: number): void;
  position(): number;
  duration(): number | null;
  dispose(): void;
  onEnded(fn: () => void): void;
}

function audioEngine(src: string): Engine {
  const el = new Audio(src);
  el.preload = "metadata";
  return {
    play: () => el.play(),
    pause: () => el.pause(),
    seek: (ms) => {
      el.currentTime = ms / 1000;
    },
    setRate: (rate) => {
      el.playbackRate = rate;
    },
    position: () => el.currentTime * 1000,
    duration: () => (Number.isFinite(el.duration) ? el.duration * 1000 : null),
    dispose: () => {
      el.pause();
      el.removeAttribute("src");
      el.load();
    },
    onEnded: (fn) => el.addEventListener("ended", fn),
  };
}

/** A silent clock with the same interface, for the browser preview. */
function clockEngine(durationMs: number): Engine {
  let base = 0;
  let since: number | null = null;
  let rate = 1;
  let ended: (() => void) | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const position = () =>
    Math.min(durationMs, base + (since === null ? 0 : (performance.now() - since) * rate));
  const schedule = () => {
    if (timer) clearTimeout(timer);
    if (since === null) return;
    timer = setTimeout(
      () => {
        base = durationMs;
        since = null;
        ended?.();
      },
      (durationMs - base) / rate,
    );
  };
  return {
    play: async () => {
      if (base >= durationMs) base = 0;
      since = performance.now();
      schedule();
    },
    pause: () => {
      base = position();
      since = null;
      if (timer) clearTimeout(timer);
    },
    seek: (ms) => {
      base = Math.max(0, Math.min(durationMs, ms));
      if (since !== null) since = performance.now();
      schedule();
    },
    setRate: (r) => {
      base = position();
      if (since !== null) since = performance.now();
      rate = r;
      schedule();
    },
    position,
    duration: () => durationMs,
    dispose: () => {
      if (timer) clearTimeout(timer);
    },
    onEnded: (fn) => {
      ended = fn;
    },
  };
}

export interface AudioPlayer {
  playing: boolean;
  positionMs: number;
  durationMs: number;
  rate: number;
  toggle: () => void;
  play: () => void;
  seek: (ms: number) => void;
  skip: (deltaMs: number) => void;
  cycleRate: () => void;
}

export function useAudioPlayer(
  src: string | null | undefined,
  fallbackDurationMs: number,
): AudioPlayer {
  const engine = useRef<Engine | null>(null);
  const [playing, setPlaying] = useState(false);
  const [positionMs, setPosition] = useState(0);
  const [durationMs, setDuration] = useState(fallbackDurationMs);
  const [rate, setRate] = useState<number>(1);

  useEffect(() => {
    if (!src) return;
    const e = src.startsWith("mock:") ? clockEngine(fallbackDurationMs) : audioEngine(src);
    engine.current = e;
    e.onEnded(() => setPlaying(false));
    setPosition(0);
    setPlaying(false);
    return () => {
      e.dispose();
      engine.current = null;
    };
  }, [src, fallbackDurationMs]);

  // Follow the playhead every frame while playing (smooth highlight); idle otherwise.
  useEffect(() => {
    if (!playing) return;
    let frame = 0;
    const tick = () => {
      const e = engine.current;
      if (e) {
        setPosition(e.position());
        const d = e.duration();
        if (d) setDuration(d);
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [playing]);

  const seek = useCallback(
    (ms: number) => {
      const e = engine.current;
      const clamped = Math.max(0, Math.min(ms, e?.duration() ?? durationMs));
      e?.seek(clamped);
      setPosition(clamped);
    },
    [durationMs],
  );

  const play = useCallback(() => {
    const e = engine.current;
    if (!e) return;
    e.play().then(
      () => setPlaying(true),
      () => setPlaying(false),
    );
  }, []);

  const toggle = useCallback(() => {
    const e = engine.current;
    if (!e) return;
    if (playing) {
      e.pause();
      setPlaying(false);
      setPosition(e.position());
    } else {
      play();
    }
  }, [playing, play]);

  const skip = useCallback(
    (delta: number) => seek((engine.current?.position() ?? 0) + delta),
    [seek],
  );

  const cycleRate = useCallback(() => {
    setRate((r) => {
      const next = RATES[(RATES.indexOf(r as (typeof RATES)[number]) + 1) % RATES.length] ?? 1;
      engine.current?.setRate(next);
      haptic("level_change");
      return next;
    });
  }, []);

  return { playing, positionMs, durationMs, rate, toggle, play, seek, skip, cycleRate };
}
