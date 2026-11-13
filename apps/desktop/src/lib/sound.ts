/**
 * Cilica's few sounds (sound-and-haptics.md), synthesised with the Web Audio API so no audio files
 * ship: two rising tones when recording starts, the same two falling when it stops, a warm
 * notes-ready chime (Chime, Bloom, Ready or Glass), an optional soft tick, and the swear jar's
 * optional clink (Settings › Swear jar). Mixed at 60% and
 * quieter than a typical notification.
 */
import type { ReadySound, SoundSettings } from "@/ipc/types";

export type SoundKind = "start" | "stop" | "ready" | "tick" | "clink";

const MASTER = 0.6;
/** Peak gain of one voice before the master; keeps every cue soft. */
const VOICE = 0.16;

let context: AudioContext | null = null;

function audio(): AudioContext | null {
  if (typeof window === "undefined") return null;
  const Ctor =
    window.AudioContext ??
    (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!Ctor) return null;
  if (!context) context = new Ctor();
  if (context.state === "suspended") void context.resume().catch(() => undefined);
  return context;
}

interface Voice {
  freq: number;
  /** Seconds after the cue starts. */
  at: number;
  /** Seconds until the voice has faded out. */
  length: number;
  type?: OscillatorType;
  gain?: number;
  /** Attack in seconds; bells use a near-instant one. */
  attack?: number;
  /** A small pitch glide (ratio of `freq`) for warmth. */
  glide?: number;
}

function play(voices: readonly Voice[]) {
  const ctx = audio();
  if (!ctx) return;
  const master = ctx.createGain();
  master.gain.value = MASTER;
  // A gentle low-pass takes the edge off sine and triangle partials.
  const tone = ctx.createBiquadFilter();
  tone.type = "lowpass";
  tone.frequency.value = 5200;
  tone.connect(master).connect(ctx.destination);
  const t0 = ctx.currentTime + 0.01;
  let end = t0;
  for (const v of voices) {
    const osc = ctx.createOscillator();
    const env = ctx.createGain();
    const start = t0 + v.at;
    const stop = start + v.length;
    const attack = v.attack ?? 0.012;
    osc.type = v.type ?? "sine";
    osc.frequency.setValueAtTime(v.freq, start);
    if (v.glide) osc.frequency.exponentialRampToValueAtTime(v.freq * v.glide, stop);
    env.gain.setValueAtTime(0.0001, start);
    env.gain.exponentialRampToValueAtTime((v.gain ?? 1) * VOICE, start + attack);
    env.gain.exponentialRampToValueAtTime(0.0001, stop);
    osc.connect(env).connect(tone);
    osc.start(start);
    osc.stop(stop + 0.02);
    end = Math.max(end, stop);
  }
  // Release the graph once the last voice has faded.
  setTimeout(() => master.disconnect(), (end - ctx.currentTime + 0.1) * 1000);
}

/** The voices for each cue. Exported for tests and previews. */
export function voicesFor(kind: SoundKind, ready: ReadySound = "chime"): Voice[] {
  switch (kind) {
    case "start":
      // Two rising soft tones, 250ms.
      return [
        { freq: 659.25, at: 0, length: 0.14, gain: 0.9 },
        { freq: 987.77, at: 0.11, length: 0.14 },
      ];
    case "stop":
      return [
        { freq: 987.77, at: 0, length: 0.14 },
        { freq: 659.25, at: 0.11, length: 0.14, gain: 0.9 },
      ];
    case "tick":
      return [{ freq: 2200, at: 0, length: 0.03, gain: 0.35, attack: 0.002 }];
    case "clink":
      // A coin landing on coins: two quick metallic pings.
      return [
        { freq: 2637, at: 0, length: 0.09, type: "triangle", gain: 0.5, attack: 0.002 },
        { freq: 3951, at: 0.045, length: 0.12, gain: 0.3, attack: 0.002, glide: 1.003 },
      ];
    case "ready":
      return readyVoices(ready);
  }
}

function readyVoices(ready: ReadySound): Voice[] {
  switch (ready) {
    case "chime":
      // One warm bell: a fundamental and a soft fifth, 400ms.
      return [
        { freq: 880, at: 0, length: 0.4, attack: 0.004 },
        { freq: 1318.5, at: 0, length: 0.32, gain: 0.35, attack: 0.004 },
        { freq: 1760, at: 0, length: 0.18, gain: 0.12, attack: 0.004 },
      ];
    case "bloom":
      // A soft major triad that swells open.
      return [
        { freq: 523.25, at: 0, length: 0.4, attack: 0.08, gain: 0.7 },
        { freq: 659.25, at: 0.04, length: 0.38, attack: 0.08, gain: 0.6 },
        { freq: 783.99, at: 0.08, length: 0.34, attack: 0.08, gain: 0.55 },
      ];
    case "ready":
      // Two bright steps up.
      return [
        { freq: 783.99, at: 0, length: 0.16, type: "triangle", gain: 0.8 },
        { freq: 1174.66, at: 0.13, length: 0.27, type: "triangle", gain: 0.8 },
      ];
    case "glass":
      // A high, glassy ping with a slightly inharmonic partial.
      return [
        { freq: 1567.98, at: 0, length: 0.4, attack: 0.003, gain: 0.7 },
        { freq: 2637, at: 0, length: 0.22, attack: 0.003, gain: 0.25, glide: 1.004 },
      ];
  }
}

/** Is this cue allowed by Settings › Sounds? */
export function soundAllowed(settings: SoundSettings | undefined, kind: SoundKind): boolean {
  if (!settings?.enabled) return false;
  switch (kind) {
    case "start":
    case "stop":
      return settings.recording_start_stop;
    case "ready":
      return settings.notes_ready;
    case "tick":
      return settings.interface_sounds;
    case "clink":
      // Its own switch lives in Settings › Swear jar; this is only the master.
      return true;
  }
}

/** Play a cue now, ignoring settings (previews in Settings › Sounds). */
export function playSound(kind: SoundKind, ready?: ReadySound) {
  try {
    play(voicesFor(kind, ready));
  } catch {
    // No audio device or the context was blocked: sounds are a nicety, never an error.
  }
}

/** Play a cue if the settings allow it. */
export function cue(settings: SoundSettings | undefined, kind: SoundKind) {
  if (soundAllowed(settings, kind)) playSound(kind, settings?.notes_ready_sound);
}
