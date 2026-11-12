/** Popovers on strong glass (speaker naming, small editors). They scale from their trigger. */
import { Popover as P } from "radix-ui";
import type { CSSProperties, ReactNode } from "react";
import { cn } from "./cn";

export const Popover = P.Root;
export const PopoverTrigger = P.Trigger;
export const PopoverAnchor = P.Anchor;
export const PopoverClose = P.Close;

export function PopoverContent({
  children,
  align = "start",
  side = "bottom",
  className,
  onOpenAutoFocus,
  label,
}: {
  children: ReactNode;
  align?: "start" | "center" | "end";
  side?: "top" | "bottom" | "left" | "right";
  className?: string;
  onOpenAutoFocus?: (event: Event) => void;
  label?: string;
}) {
  return (
    <P.Portal>
      <P.Content
        align={align}
        side={side}
        sideOffset={6}
        collisionPadding={8}
        aria-label={label}
        onOpenAutoFocus={onOpenAutoFocus}
        className={cn("nt-menu nt-glass-strong nt-menu-layer nt-popover", className)}
        style={{ "--origin": "var(--radix-popover-content-transform-origin)" } as CSSProperties}
      >
        {children}
      </P.Content>
    </P.Portal>
  );
}
