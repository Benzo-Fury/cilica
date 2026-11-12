import type { LucideIcon } from "lucide-react";
import type { ButtonHTMLAttributes, Ref } from "react";
import { cn } from "./cn";
import { Icon, type IconMotion } from "./Icon";
import { Tooltip } from "./Tooltip";

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon: LucideIcon;
  /** Required: the accessible name and the tooltip text. */
  label: string;
  iconMotion?: IconMotion;
  shortcut?: readonly string[];
  /** Toggle buttons: the icon turns `accent-text` while on. */
  pressed?: boolean;
  size?: "md" | "sm";
  tooltipSide?: "top" | "bottom" | "left" | "right";
  /** Skip the tooltip (when an outer component provides one). */
  noTooltip?: boolean;
  ref?: Ref<HTMLButtonElement>;
}

/** Icon-only control: just the glyph until hover. 28px hit area, never smaller. */
export function IconButton({
  icon,
  label,
  iconMotion,
  shortcut,
  pressed,
  size = "md",
  tooltipSide = "bottom",
  noTooltip = false,
  className,
  type = "button",
  ...props
}: IconButtonProps) {
  const button = (
    <button
      type={type}
      aria-label={label}
      aria-pressed={pressed}
      className={cn("nt-iconbtn nt-anim", size === "sm" && "nt-iconbtn-sm", className)}
      {...props}
    >
      <Icon icon={icon} motion={iconMotion} size={size === "sm" ? "sm" : "md"} />
    </button>
  );
  return (
    <Tooltip label={label} shortcut={shortcut} side={tooltipSide} enabled={!noTooltip}>
      {button}
    </Tooltip>
  );
}
