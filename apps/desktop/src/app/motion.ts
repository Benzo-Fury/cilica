/**
 * Motion presets, exactly as docs/design-system/motion.md.
 * Springs carry position, size and scale; short tweens carry colour and opacity.
 * The matching CSS curves are the `--spring-*` / `--dur-*` variables in components.css.
 */
import type { Transition } from "motion/react";

export const springs = {
  /** Press release, hover nudges, chevrons, checkbox, menus opening, icon motions. */
  snappy: { type: "spring", visualDuration: 0.2, bounce: 0.15 },
  /** Panels and sidebar collapse, segmented thumb, active-row pill, palette and dialog open. */
  smooth: { type: "spring", visualDuration: 0.3, bounce: 0.1 },
  /** Record button morph, capsule widening, toasts entering, drag-and-drop settling, switch knob. */
  bouncy: { type: "spring", visualDuration: 0.35, bounce: 0.3 },
  /** View changes: Notes ⇄ Transcript, opening a note, settings pages. */
  gentle: { type: "spring", visualDuration: 0.45, bounce: 0 },
} as const satisfies Record<string, Transition>;

export const durations = {
  instant: 0.08,
  fast: 0.12,
  base: 0.18,
  slow: 0.28,
} as const;

export const easings = {
  out: [0.22, 1, 0.36, 1],
  in: [0.55, 0, 1, 0.45],
  inOut: [0.65, 0, 0.35, 1],
} as const satisfies Record<string, [number, number, number, number]>;

export const tweens = {
  /** Hover colour and wash, tooltip in. */
  fast: { duration: durations.fast, ease: easings.out },
  /** Fades, focus halo, scrim. */
  base: { duration: durations.base, ease: easings.out },
  /** Every exit: quick, never bounces. */
  exit: { duration: durations.fast, ease: easings.in },
  /** Frequently repeated moves (palette and menu highlight). */
  highlight: { duration: 0.14, ease: easings.out },
} as const satisfies Record<string, Transition>;

/** Reduced motion: travel becomes a 120ms linear crossfade. */
export const reducedTransition: Transition = { duration: durations.fast, ease: "linear" };

/** The view-change transition (Notes ⇄ Transcript, settings pages): crossfade plus an 8px slide. */
export function viewSlide(direction: 1 | -1, reduced: boolean) {
  const x = reduced ? 0 : 8 * direction;
  return {
    initial: { opacity: 0, x },
    animate: { opacity: 1, x: 0, transition: reduced ? reducedTransition : springs.gentle },
    exit: { opacity: 0, x: -x, transition: reduced ? reducedTransition : tweens.exit },
  };
}
