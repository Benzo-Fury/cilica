import { Check, type LucideIcon, X } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";
import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode, Ref } from "react";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { Tooltip } from "./Tooltip";

export type ChipTone = "neutral" | "accent" | "rec" | "success" | "warning" | "danger";

interface BaseProps {
  tone?: ChipTone;
  icon?: LucideIcon;
  /** Leading element in place of an icon (avatar stack, rec dot). */
  leading?: ReactNode;
  className?: string;
  children?: ReactNode;
}

const toneClass = (tone: ChipTone) => (tone === "neutral" ? null : `nt-chip-${tone}`);

export type ChipProps = BaseProps & HTMLAttributes<HTMLSpanElement>;

/** Small pill for metadata and status. Never more than one icon. */
export function Chip({
  tone = "neutral",
  icon,
  leading,
  className,
  children,
  ...props
}: ChipProps) {
  return (
    <span className={cn("nt-chip", toneClass(tone), className)} {...props}>
      {leading ?? (icon && <Icon icon={icon} size="sm" />)}
      {children}
    </span>
  );
}

export type ChipButtonProps = BaseProps & ButtonHTMLAttributes<HTMLButtonElement>;

/** An interactive chip: darkens one step on hover. */
export function ChipButton({
  tone = "neutral",
  icon,
  leading,
  className,
  children,
  type = "button",
  ...props
}: ChipButtonProps) {
  return (
    <button type={type} className={cn("nt-chip nt-anim", toneClass(tone), className)} {...props}>
      {leading ?? (icon && <Icon icon={icon} size="sm" />)}
      {children}
    </button>
  );
}

/** The 6px breathing recording dot used inside chips and rows. */
export function RecDot({ size = "md", still = false }: { size?: "sm" | "md"; still?: boolean }) {
  return (
    <span
      aria-hidden
      className={cn("nt-rec-dot", size === "sm" && "nt-rec-dot-sm", still && "nt-rec-dot-still")}
    />
  );
}

/** A count badge for sidebar rows ("1 proposal"). */
export function CountBadge({ children }: { children: ReactNode }) {
  return <span className="nt-chip nt-chip-accent nt-chip-badge">{children}</span>;
}

export interface SuggestionChipProps {
  /** The one icon, before the label. */
  icon?: LucideIcon;
  children: ReactNode;
  /** Why it is suggested, shown on hover. */
  hint?: ReactNode;
  acceptLabel: string;
  dismissLabel: string;
  onAccept: () => void;
  onDismiss: () => void;
  disabled?: boolean;
  /** `AnimatePresence mode="popLayout"` measures the chip through this. */
  ref?: Ref<HTMLSpanElement>;
}

/**
 * A suggestion waiting for a yes or no: a dashed chip with Accept and Dismiss. Springs in, fades
 * out (wrap a list of them in `AnimatePresence`).
 */
export function SuggestionChip({
  icon,
  children,
  hint,
  acceptLabel,
  dismissLabel,
  onAccept,
  onDismiss,
  disabled,
  ref,
}: SuggestionChipProps) {
  const reduced = useReducedMotion();
  const label = (
    <span className="nt-chip-suggest-label">
      {icon && <Icon icon={icon} size="sm" />}
      <span className="nt-chip-suggest-text">{children}</span>
    </span>
  );
  return (
    <motion.span
      ref={ref}
      layout={!reduced}
      className="nt-chip nt-chip-suggest"
      initial={reduced ? { opacity: 0 } : { opacity: 0, scale: 0.92 }}
      animate={{ opacity: 1, scale: 1, transition: reduced ? reducedTransition : springs.snappy }}
      exit={{ opacity: 0, scale: reduced ? 1 : 0.96, transition: tweens.exit }}
    >
      {hint ? (
        <Tooltip label={<span className="nt-tip-text">{hint}</span>}>
          {/* biome-ignore lint/a11y/noNoninteractiveTabindex: the hint must be reachable by keyboard */}
          <span tabIndex={0} className="nt-chip-suggest-focus">
            {label}
          </span>
        </Tooltip>
      ) : (
        label
      )}
      <Tooltip label={acceptLabel}>
        <button
          type="button"
          className="nt-chip-suggest-act nt-anim"
          aria-label={acceptLabel}
          disabled={disabled}
          onClick={onAccept}
        >
          <Icon icon={Check} size="sm" />
        </button>
      </Tooltip>
      <Tooltip label={dismissLabel}>
        <button
          type="button"
          className="nt-chip-suggest-act nt-anim"
          aria-label={dismissLabel}
          disabled={disabled}
          onClick={onDismiss}
        >
          <Icon icon={X} size="sm" />
        </button>
      </Tooltip>
    </motion.span>
  );
}
