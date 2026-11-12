/**
 * The recorder capsule's parts (RecorderCapsule doc): a glass pill docked bottom-centre of the
 * canvas whose width springs (`bouncy`) as its content changes, the Stop button that morphs to
 * a check, and the idle "Start recording" label. The feature decides what goes inside.
 */
import { motion, useReducedMotion } from "motion/react";
import type { ButtonHTMLAttributes, ReactNode } from "react";
import { reducedTransition, springs } from "@/app/motion";
import { cn } from "./cn";
import { useSpecular } from "./useSpecular";

export function CapsuleDock({ children }: { children: ReactNode }) {
  return <div className="nt-capsule-dock">{children}</div>;
}

/** The glass pill. Its size animates with Motion `layout` so content can change freely. */
export function Capsule({
  children,
  className,
  label,
  live = false,
}: {
  children: ReactNode;
  className?: string;
  label: string;
  /** Recording: the pill reveals its extra controls on hover or focus. */
  live?: boolean;
}) {
  const reduced = useReducedMotion();
  const specular = useSpecular<HTMLDivElement>();
  return (
    <motion.div
      layout
      role="group"
      aria-label={label}
      className={cn(
        "nt-capsule nt-glass nt-specular nt-capsule-app",
        live && "nt-capsule-live",
        className,
      )}
      style={{ borderRadius: 999 }}
      transition={reduced ? reducedTransition : springs.bouncy}
      onPointerMove={specular}
    >
      {children}
    </motion.div>
  );
}

/** One state's content inside the capsule; crossfades and keeps its position while the pill resizes. */
export function CapsuleContent({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  const reduced = useReducedMotion();
  return (
    <motion.div
      layout="position"
      className={cn("nt-capsule-content", className)}
      initial={{ opacity: 0, filter: reduced ? "none" : "blur(4px)" }}
      animate={{
        opacity: 1,
        filter: "blur(0px)",
        transition: { duration: 0.18, ease: [0.22, 1, 0.36, 1] },
      }}
      exit={{ opacity: 0, transition: { duration: 0.1, ease: [0.55, 0, 1, 0.45] } }}
      transition={reduced ? reducedTransition : springs.bouncy}
    >
      {children}
    </motion.div>
  );
}

/** The recording dot as a shared element: it travels from the idle label to the live timer. */
export function CapsuleDot({ still = false }: { still?: boolean }) {
  const reduced = useReducedMotion();
  return (
    <motion.span
      layoutId="capsule-dot"
      className={cn("nt-rec-dot", still && "nt-rec-dot-still")}
      transition={reduced ? reducedTransition : springs.bouncy}
      aria-hidden
    />
  );
}

export interface StopButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** After Stop: the square morphs into a tick and the button goes quiet. */
  done?: boolean;
  size?: "md" | "lg";
}

/** `rec` round button with a white square; morphs to a drawn check once stopping. */
export function StopButton({ done = false, size = "md", className, ...props }: StopButtonProps) {
  return (
    <button
      type="button"
      className={cn(
        "nt-stop nt-stop-app",
        size === "lg" && "nt-stop-lg",
        done && "nt-stop-done",
        className,
      )}
      {...props}
    >
      <svg viewBox="0 0 12 12" aria-hidden className="nt-stop-check">
        <path d="M2.5 6.5l2.3 2.3 4.7-5" />
      </svg>
    </button>
  );
}

/** A tabular, monospaced timer. */
export function Timecode({ children, className }: { children: ReactNode; className?: string }) {
  return <span className={cn("nt-mono nt-timer", className)}>{children}</span>;
}
