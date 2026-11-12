import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useRef } from "react";
import { reducedTransition, springs, tweens } from "@/app/motion";

/** A count that rolls vertically when it changes (up when it grows). Tabular figures. */
export function RollingNumber({ value }: { value: number }) {
  const previous = useRef(value);
  const direction = value >= previous.current ? 1 : -1;
  previous.current = value;
  const reduced = useReducedMotion();
  return (
    <span className="nt-roll">
      <span className="nt-sr-only">{value}</span>
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span
          key={value}
          initial={{ y: reduced ? 0 : 12 * direction, opacity: 0 }}
          animate={{ y: 0, opacity: 1, transition: reduced ? reducedTransition : springs.bouncy }}
          exit={{ y: reduced ? 0 : -12 * direction, opacity: 0, transition: tweens.exit }}
          aria-hidden
        >
          {value}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}
