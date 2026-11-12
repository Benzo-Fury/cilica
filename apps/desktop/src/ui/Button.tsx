import type { LucideIcon } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import type { ButtonHTMLAttributes, Ref } from "react";
import { tweens } from "@/app/motion";
import { cn } from "./cn";
import { Icon, type IconMotion } from "./Icon";
import { Kbd } from "./Kbd";
import { Spinner } from "./Spinner";
import { useSpecular } from "./useSpecular";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "glass" | "danger";
export type ButtonSize = "sm" | "md" | "lg";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: LucideIcon;
  iconMotion?: IconMotion;
  /** Trailing shortcut keycaps (dialogs, cards, empty states). */
  shortcut?: readonly string[];
  /** Keeps the button's size; the label crossfades to a spinner. */
  loading?: boolean;
  ref?: Ref<HTMLButtonElement>;
}

/** Buttons trigger one action; the label is the verb that happens. One primary per surface. */
export function Button({
  variant = "secondary",
  size = "md",
  icon,
  iconMotion,
  shortcut,
  loading = false,
  className,
  children,
  disabled,
  onPointerMove,
  type = "button",
  ...props
}: ButtonProps) {
  const specular = useSpecular<HTMLButtonElement>();
  return (
    <button
      type={type}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      className={cn(
        "nt-btn nt-anim",
        `nt-btn-${variant}`,
        size === "sm" && "nt-btn-sm",
        size === "lg" && "nt-btn-lg",
        className,
      )}
      onPointerMove={(e) => {
        if (variant === "glass") specular(e);
        onPointerMove?.(e);
      }}
      {...props}
    >
      <span className={cn("nt-btn-label", loading && "nt-btn-label-hidden")}>
        {icon && <Icon icon={icon} size={size === "lg" ? "md" : "sm"} motion={iconMotion} />}
        {children}
        {shortcut && <Kbd keys={shortcut} tone={variant === "primary" ? "on-accent" : "default"} />}
      </span>
      <AnimatePresence>
        {loading && (
          <motion.span
            className="nt-btn-spinner"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: tweens.base }}
            exit={{ opacity: 0, transition: tweens.exit }}
          >
            <Spinner />
          </motion.span>
        )}
      </AnimatePresence>
    </button>
  );
}
