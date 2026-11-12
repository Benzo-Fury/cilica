/**
 * Sidebar-style rows: 32px, `radius-sm`, `ink-2` at rest, `fill-hover` on hover. The current
 * row's `fill-press` pill is one shared element that slides between rows (`ActivePill`).
 */
import type { LucideIcon } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";
import { type HTMLAttributes, type ReactNode, type Ref, useEffect, useRef } from "react";
import { reducedTransition, springs } from "@/app/motion";
import { cn } from "./cn";
import { Icon, type IconMotion } from "./Icon";

export interface RowProps extends HTMLAttributes<HTMLDivElement> {
  icon?: LucideIcon;
  iconMotion?: IconMotion;
  /** Leading element instead of an icon (rec dot, chevron + folder). */
  leading?: ReactNode;
  label: ReactNode;
  meta?: ReactNode;
  trailing?: ReactNode;
  current?: boolean;
  indent?: boolean;
  /** Visual states used while dragging. */
  dropTarget?: boolean;
  dimmed?: boolean;
  /** The layout id of the sliding selection pill (one per list). */
  pillId?: string;
  /** Shows the drop insert line at this edge while dragging. */
  dropLine?: "before" | "after" | null;
  ref?: Ref<HTMLDivElement>;
}

/** A selectable row. Pass `role`/`tabIndex`/handlers for interactivity. */
export function Row({
  icon,
  iconMotion,
  leading,
  label,
  meta,
  trailing,
  current = false,
  indent = false,
  dropTarget = false,
  dimmed = false,
  pillId,
  dropLine = null,
  className,
  ...props
}: RowProps) {
  return (
    <div
      aria-current={current ? "page" : undefined}
      className={cn(
        "nt-row nt-anim-host nt-anim",
        indent && "nt-row-indent",
        dropTarget && "nt-drop-target",
        dimmed && "nt-row-dimmed",
        className,
      )}
      {...props}
    >
      {current && pillId && <ActivePill layoutId={pillId} />}
      {dropLine && (
        <span
          className={cn(
            "nt-drop-line nt-drop-line-abs",
            dropLine === "after" && "nt-drop-line-after",
          )}
          aria-hidden
        />
      )}
      {leading ?? (icon && <Icon icon={icon} motion={iconMotion} />)}
      <span className="nt-row-label">{label}</span>
      {meta !== undefined && meta !== null && <span className="nt-row-meta">{meta}</span>}
      {trailing}
    </div>
  );
}

/**
 * The sliding selection pill. Render it inside the current row only. It slides when it arrives
 * in a row; after that it stays glued to the row, so a moving or resizing panel never leaves
 * it trailing behind.
 */
export function ActivePill({ layoutId }: { layoutId: string }) {
  const reduced = useReducedMotion();
  const arrived = useRef(false);
  useEffect(() => {
    arrived.current = true;
  }, []);
  return (
    <motion.span
      layoutId={layoutId}
      layout="position"
      className="nt-row-pill"
      transition={arrived.current ? { duration: 0 } : reduced ? reducedTransition : springs.smooth}
      aria-hidden
    />
  );
}

/** Uppercase section caption ("Categories") with an optional action on the right. */
export function SectionLabel({
  children,
  action,
  className,
  ...props
}: HTMLAttributes<HTMLDivElement> & { action?: ReactNode }) {
  return (
    <div className={cn("nt-side-label", className)} {...props}>
      <span>{children}</span>
      {action}
    </div>
  );
}

/** A chevron that rotates 90° when its parent has `aria-expanded="true"`. */
export function Chevron() {
  return (
    <svg
      className="nt-ico nt-ico-sm nt-chevron"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="m9 18 6-6-6-6" />
    </svg>
  );
}

/** The 2px accent insert line with a ring that marks a drop position. */
export function DropLine({ indent = false }: { indent?: boolean }) {
  return <div className={cn("nt-drop-line", indent && "nt-drop-line-indent")} aria-hidden />;
}
