/**
 * Menus: dropdowns (`Menu*`) and context menus (`ContextMenu*`), both on Radix with the same look.
 * Strong glass, 30px items, shortcuts right-aligned, destructive items last after a separator.
 * They open from the trigger or pointer (Radix transform origin) with `--spring-snappy`.
 */
import type { LucideIcon } from "lucide-react";
import { ContextMenu as CM, DropdownMenu as DM } from "radix-ui";
import type { CSSProperties, ReactNode } from "react";
import { cn } from "./cn";
import { Icon, type IconMotion } from "./Icon";
import { Kbd } from "./Kbd";

interface ItemProps {
  icon?: LucideIcon;
  iconMotion?: IconMotion;
  shortcut?: readonly string[];
  danger?: boolean;
  disabled?: boolean;
  onSelect?: (event: Event) => void;
  children: ReactNode;
}

function ItemBody({ icon, iconMotion, shortcut, children }: ItemProps) {
  return (
    <>
      {icon ? <Icon icon={icon} size="sm" motion={iconMotion} /> : <span className="nt-menu-gap" />}
      <span className="nt-menu-label">{children}</span>
      {shortcut && <Kbd keys={shortcut} />}
    </>
  );
}

const itemClass = (danger?: boolean) =>
  cn("nt-menu-item nt-anim-host nt-anim", danger && "nt-menu-item-danger");

// ───────── Dropdown ─────────

export const Menu = DM.Root;
export const MenuTrigger = DM.Trigger;

export function MenuContent({
  children,
  align = "end",
  side = "bottom",
  className,
}: {
  children: ReactNode;
  align?: "start" | "center" | "end";
  side?: "top" | "bottom" | "left" | "right";
  className?: string;
}) {
  return (
    <DM.Portal>
      <DM.Content
        align={align}
        side={side}
        sideOffset={6}
        collisionPadding={8}
        loop
        className={cn("nt-menu nt-glass-strong nt-menu-layer", className)}
        style={
          { "--origin": "var(--radix-dropdown-menu-content-transform-origin)" } as CSSProperties
        }
      >
        {children}
      </DM.Content>
    </DM.Portal>
  );
}

export function MenuItem({ danger, disabled, onSelect, ...rest }: ItemProps) {
  return (
    <DM.Item className={itemClass(danger)} disabled={disabled} onSelect={onSelect}>
      <ItemBody {...rest} />
    </DM.Item>
  );
}

export function MenuSeparator() {
  return <DM.Separator className="nt-menu-sep" />;
}

export function MenuLabel({ children }: { children: ReactNode }) {
  return <DM.Label className="nt-menu-heading">{children}</DM.Label>;
}

export function MenuSub({
  label,
  icon,
  iconMotion,
  children,
}: {
  label: string;
  icon?: LucideIcon;
  iconMotion?: IconMotion;
  children: ReactNode;
}) {
  return (
    <DM.Sub>
      <DM.SubTrigger className={cn(itemClass(), "nt-menu-subtrigger")}>
        <ItemBody icon={icon} iconMotion={iconMotion}>
          {label}
        </ItemBody>
      </DM.SubTrigger>
      <DM.Portal>
        <DM.SubContent
          sideOffset={4}
          collisionPadding={8}
          className="nt-menu nt-glass-strong nt-menu-layer"
          style={
            { "--origin": "var(--radix-dropdown-menu-content-transform-origin)" } as CSSProperties
          }
        >
          {children}
        </DM.SubContent>
      </DM.Portal>
    </DM.Sub>
  );
}

// ───────── Context menu ─────────

export const ContextMenu = CM.Root;
export const ContextMenuTrigger = CM.Trigger;

export function ContextMenuContent({ children }: { children: ReactNode }) {
  return (
    <CM.Portal>
      <CM.Content
        collisionPadding={8}
        loop
        className="nt-menu nt-glass-strong nt-menu-layer"
        style={
          { "--origin": "var(--radix-context-menu-content-transform-origin)" } as CSSProperties
        }
      >
        {children}
      </CM.Content>
    </CM.Portal>
  );
}

export function ContextMenuItem({ danger, disabled, onSelect, ...rest }: ItemProps) {
  return (
    <CM.Item className={itemClass(danger)} disabled={disabled} onSelect={onSelect}>
      <ItemBody {...rest} />
    </CM.Item>
  );
}

export function ContextMenuSeparator() {
  return <CM.Separator className="nt-menu-sep" />;
}

export function ContextMenuSub({
  label,
  icon,
  iconMotion,
  children,
}: {
  label: string;
  icon?: LucideIcon;
  iconMotion?: IconMotion;
  children: ReactNode;
}) {
  return (
    <CM.Sub>
      <CM.SubTrigger className={cn(itemClass(), "nt-menu-subtrigger")}>
        <ItemBody icon={icon} iconMotion={iconMotion}>
          {label}
        </ItemBody>
      </CM.SubTrigger>
      <CM.Portal>
        <CM.SubContent
          sideOffset={4}
          collisionPadding={8}
          className="nt-menu nt-glass-strong nt-menu-layer"
          style={
            { "--origin": "var(--radix-context-menu-content-transform-origin)" } as CSSProperties
          }
        >
          {children}
        </CM.SubContent>
      </CM.Portal>
    </CM.Sub>
  );
}
