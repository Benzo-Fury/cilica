import type { LucideIcon, LucideProps } from "lucide-react";
import { cn } from "./cn";

/**
 * One-shot hover motions from bundle.css (`nt-i-*`). They play when the host control
 * (Button, IconButton, Row, MenuItem, or any `.nt-anim-host`) is hovered or focused.
 */
export type IconMotion =
  | "plus"
  | "search"
  | "gear"
  | "arrow"
  | "up"
  | "bell"
  | "trash"
  | "mic"
  | "copy"
  | "undo"
  | "check"
  | "folder"
  | "sidebar"
  | "clock"
  | "users"
  | "x";

export interface IconProps extends Omit<LucideProps, "ref"> {
  icon: LucideIcon;
  /** 18px by default, 16px in chips, menus and dense rows. */
  size?: "md" | "sm";
  motion?: IconMotion;
}

/** A Lucide glyph at 1.75 stroke, always `currentColor`, never boxed. */
export function Icon({ icon: Glyph, size = "md", motion, className, ...props }: IconProps) {
  return (
    <Glyph
      aria-hidden
      strokeWidth={1.75}
      className={cn("nt-ico", size === "sm" && "nt-ico-sm", motion && `nt-i-${motion}`, className)}
      {...props}
    />
  );
}
