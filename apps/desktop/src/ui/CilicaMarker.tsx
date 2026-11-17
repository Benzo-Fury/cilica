/**
 * Where Cilica is working in a note: a soft wash over the section and a small dark badge on its
 * top edge ("Cilica is editing here"). When the target moves, both spring there quickly, like
 * someone moving around your screen. Only transform and opacity animate; size changes ride the
 * layout animation (transform). Reduce motion: they crossfade in place instead of travelling.
 */
import type { LucideIcon } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { Icon } from "./Icon";

export interface MarkerBox {
  top: number;
  left: number;
  width: number;
  height: number;
}

export function CilicaMarker({
  box,
  label,
  icon,
}: {
  box: MarkerBox | null;
  label: string;
  icon: LucideIcon;
}) {
  const reduced = useReducedMotion() ?? false;
  const travel = reduced ? reducedTransition : springs.snappy;
  // Reduce motion: a new place is a new element that fades in, instead of one that travels.
  const key = reduced && box ? `${Math.round(box.top)}:${Math.round(box.height)}` : "marker";
  return (
    <AnimatePresence>
      {box && (
        <motion.div
          key={key}
          className="nt-cilica-mark-layer"
          aria-hidden
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: tweens.base }}
          exit={{ opacity: 0, transition: tweens.exit }}
        >
          <motion.div
            layout={!reduced}
            className="nt-cilica-mark"
            style={{ top: box.top, left: box.left, width: box.width, height: box.height }}
            transition={travel}
          />
          <motion.span
            layout={reduced ? false : "position"}
            className="nt-cilica-mark-badge"
            style={{ top: box.top, left: box.left + box.width }}
            transition={travel}
          >
            <Icon icon={icon} size="sm" />
            {label}
          </motion.span>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
