import type { LucideIcon } from "lucide-react";
import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";
import { Icon } from "./Icon";

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
