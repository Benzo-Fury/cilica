/**
 * The launch intro. The window opens on the empty page alone (`hold`); once its face has been on
 * screen for HOLD_MS, the band and floating controls come in (`reveal`), then the intro
 * steps aside (`done`). It runs once per launch.
 */
import { useEffect } from "react";
import { create } from "zustand";

export type IntroPhase = "hold" | "reveal" | "done";

/** How long the empty page stands alone once its face is showing. */
export const HOLD_MS = 1500;
/** Reveal anyway if the page never reports its face (a slow load, another view). */
const FALLBACK_MS = 3000;
/** Long enough for the staggered fades to finish at launch speed. */
const REVEAL_MS = 1400;
/**
 * The opening plays a little slower than everyday motion: springs started during the intro run
 * this much longer (the CSS side stretches `--spring-gentle-dur` to match in ui.css).
 */
export const LAUNCH_SLOWDOWN = 1.4;

interface IntroState {
  phase: IntroPhase;
  /** The empty page's face (or the welcome) is on screen. */
  faceShown: boolean;
  showFace: () => void;
  reveal: () => void;
  settle: () => void;
}

export const useIntro = create<IntroState>()((set, get) => ({
  phase: "hold",
  faceShown: false,
  showFace: () => {
    if (!get().faceShown) set({ faceShown: true });
  },
  reveal: () => {
    if (get().phase === "hold") set({ phase: "reveal" });
  },
  settle: () => set({ phase: "done" }),
}));

/** True while the empty page is holding the window alone. */
export function useIntroHolding(): boolean {
  return useIntro((s) => s.phase === "hold");
}

/** Drives the phases; mount once, in the shell. */
export function useLaunchIntro(): IntroPhase {
  const phase = useIntro((s) => s.phase);
  const faceShown = useIntro((s) => s.faceShown);

  // A ceiling, so the chrome never stays hidden.
  useEffect(() => {
    const t = setTimeout(useIntro.getState().reveal, FALLBACK_MS);
    return () => clearTimeout(t);
  }, []);

  // The hold starts when the face appears, so the whole 1.5s is spent reading it.
  useEffect(() => {
    if (!faceShown) return;
    const t = setTimeout(useIntro.getState().reveal, HOLD_MS);
    return () => clearTimeout(t);
  }, [faceShown]);

  useEffect(() => {
    if (phase !== "reveal") return;
    const t = setTimeout(useIntro.getState().settle, REVEAL_MS);
    return () => clearTimeout(t);
  }, [phase]);

  return phase;
}
