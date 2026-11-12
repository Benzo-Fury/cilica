import { Tooltip as T } from "radix-ui";
import type { ReactElement, ReactNode } from "react";
import { Kbd } from "./Kbd";

/** Mount once near the root: 500ms first delay, instant between neighbours within 300ms. */
export function TooltipProvider({ children }: { children: ReactNode }) {
  return (
    <T.Provider delayDuration={500} skipDelayDuration={300}>
      {children}
    </T.Provider>
  );
}

export interface TooltipProps {
  label: ReactNode;
  shortcut?: readonly string[];
  side?: "top" | "bottom" | "left" | "right";
  align?: "start" | "center" | "end";
  /** Only the trigger is rendered when false. */
  enabled?: boolean;
  delay?: number;
  children: ReactElement;
}

/** Names an icon button and shows its shortcut. Inverted, 12px, 8px from the trigger. */
export function Tooltip({
  label,
  shortcut,
  side = "bottom",
  align = "center",
  enabled = true,
  delay,
  children,
}: TooltipProps) {
  if (!enabled) return children;
  return (
    <T.Root delayDuration={delay}>
      <T.Trigger asChild>{children}</T.Trigger>
      <T.Portal>
        <T.Content
          side={side}
          align={align}
          sideOffset={8}
          collisionPadding={8}
          className="nt-tip nt-tip-layer"
        >
          {label}
          {shortcut && shortcut.length > 0 && <Kbd keys={shortcut} />}
        </T.Content>
      </T.Portal>
    </T.Root>
  );
}
