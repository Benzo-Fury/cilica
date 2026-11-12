/**
 * A quiet ambient pad for the setup window, made with Web Audio (no file to ship). It starts
 * muted: Margin plays nothing unasked (docs/design-system/sound-and-haptics.md). The speaker
 * button in the title band turns it on; it fades out when setup closes.
 */
import { useCallback, useEffect, useRef, useState } from "react";

/** A soft A-major voicing, each voice breathing at its own slow rate. */
const VOICES = [110, 164.81, 220, 277.18, 329.63];
const LEVEL = 0.09;

interface Pad {
  ctx: AudioContext;
  master: GainNode;
}

function createPad(): Pad | null {
  const Ctor =
    window.AudioContext ??
    (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!Ctor) return null;
  const ctx = new Ctor();
  const master = ctx.createGain();
  master.gain.value = 0;
  const lowpass = ctx.createBiquadFilter();
  lowpass.type = "lowpass";
  lowpass.frequency.value = 900;
  lowpass.connect(master);
  master.connect(ctx.destination);
  VOICES.forEach((frequency, i) => {
    const osc = ctx.createOscillator();
    osc.type = i % 2 ? "triangle" : "sine";
    osc.frequency.value = frequency;
    osc.detune.value = (i - 2) * 4;
    const gain = ctx.createGain();
    gain.gain.value = 0.12;
    const lfo = ctx.createOscillator();
    lfo.frequency.value = 0.05 + i * 0.03;
    const depth = ctx.createGain();
    depth.gain.value = 0.08;
    lfo.connect(depth);
    depth.connect(gain.gain);
    osc.connect(gain);
    gain.connect(lowpass);
    osc.start();
    lfo.start();
  });
  return { ctx, master };
}

export function useAmbientSound() {
  const pad = useRef<Pad | null>(null);
  const [playing, setPlaying] = useState(false);

  const toggle = useCallback(() => {
    setPlaying((on) => {
      const next = !on;
      if (next && !pad.current) pad.current = createPad();
      const p = pad.current;
      if (p) {
        if (next) void p.ctx.resume();
        p.master.gain.cancelScheduledValues(p.ctx.currentTime);
        p.master.gain.setTargetAtTime(next ? LEVEL : 0, p.ctx.currentTime, next ? 0.8 : 0.25);
      }
      return next;
    });
  }, []);

  const fadeOut = useCallback(() => {
    const p = pad.current;
    if (p) p.master.gain.setTargetAtTime(0, p.ctx.currentTime, 0.25);
    setPlaying(false);
  }, []);

  useEffect(
    () => () => {
      void pad.current?.ctx.close();
      pad.current = null;
    },
    [],
  );

  return { playing, toggle, fadeOut };
}
