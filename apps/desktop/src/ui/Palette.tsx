/**
 * Command palette pieces on top of cmdk: a 52px input row, grouped 36px rows with one sliding
 * highlight (140ms ease-out, no spring), and a footer of key hints. Matched characters are bold.
 */
import { Command } from "cmdk";
import type { LucideIcon } from "lucide-react";
import { motion } from "motion/react";
import type { ReactNode, Ref } from "react";
import { tweens } from "@/app/motion";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { Kbd } from "./Kbd";

export const PaletteRoot = Command;

export function PaletteInput({
  value,
  onValueChange,
  placeholder,
  icon,
  escKeys,
  ref,
}: {
  value: string;
  onValueChange: (value: string) => void;
  placeholder: string;
  icon: LucideIcon;
  /** The platform's Esc keycap. */
  escKeys: readonly string[];
  ref?: Ref<HTMLInputElement>;
}) {
  return (
    <div className="nt-palette-input">
      <Icon icon={icon} />
      <Command.Input
        ref={ref}
        value={value}
        onValueChange={onValueChange}
        placeholder={placeholder}
      />
      <Kbd keys={escKeys} />
    </div>
  );
}

export function PaletteList({ children }: { children: ReactNode }) {
  return <Command.List className="nt-palette-list nt-palette-scroll">{children}</Command.List>;
}

export function PaletteEmpty({ children }: { children: ReactNode }) {
  return <Command.Empty className="nt-palette-empty">{children}</Command.Empty>;
}

/** A plain line between groups, for when a search finds nothing but there is still something to say. */
export function PaletteNote({ children }: { children: ReactNode }) {
  return (
    <div className="nt-palette-note" aria-live="polite">
      {children}
    </div>
  );
}

/** Words as someone said them, set in the italic serif used for quotes. */
export function PalettePhrase({ children }: { children: ReactNode }) {
  return <span className="nt-palette-phrase">{children}</span>;
}

export function PaletteGroup({ heading, children }: { heading: string; children: ReactNode }) {
  return (
    <Command.Group heading={heading} className="nt-palette-grp">
      {children}
    </Command.Group>
  );
}

export interface PaletteItemProps {
  value: string;
  selected: boolean;
  onSelect: () => void;
  icon?: LucideIcon;
  leading?: ReactNode;
  children: ReactNode;
  hint?: ReactNode;
  shortcut?: readonly string[];
  keywords?: string[];
  /** A second line under the label (an excerpt). */
  sub?: ReactNode;
}

export function PaletteItem({
  value,
  selected,
  onSelect,
  icon,
  leading,
  children,
  hint,
  shortcut,
  keywords,
  sub,
}: PaletteItemProps) {
  return (
    <Command.Item
      value={value}
      keywords={keywords}
      onSelect={onSelect}
      className={cn("nt-prow nt-anim-host nt-anim", sub ? "nt-prow-tall" : undefined)}
    >
      {selected && (
        <motion.span
          layoutId="palette-highlight"
          className="nt-prow-hl"
          transition={tweens.highlight}
          aria-hidden
        />
      )}
      {leading ?? (icon && <Icon icon={icon} />)}
      <span className="nt-prow-label">
        {children}
        {sub && <span className="nt-prow-sub">{sub}</span>}
      </span>
      {(hint || shortcut) && (
        <span className="nt-prow-hint">
          {shortcut && shortcut.length > 0 ? <Kbd keys={shortcut} /> : hint}
        </span>
      )}
    </Command.Item>
  );
}

export function PaletteFooter({
  hints,
}: {
  hints: readonly { keys: readonly string[]; label: string }[];
}) {
  return (
    <div className="nt-palette-foot">
      {hints.map((h) => (
        <span key={h.label}>
          <Kbd keys={h.keys} />
          {h.label}
        </span>
      ))}
    </div>
  );
}

/** Bold the characters of `text` that match `query` (case-insensitive substring first). */
export function Highlight({
  text,
  query,
  className,
}: {
  text: string;
  query: string;
  className?: string;
}) {
  const q = query.trim();
  if (!q) return <span className={className}>{text}</span>;
  const index = text.toLowerCase().indexOf(q.toLowerCase());
  if (index < 0) return <span className={className}>{text}</span>;
  return (
    <span className={cn(className)}>
      {text.slice(0, index)}
      <b>{text.slice(index, index + q.length)}</b>
      {text.slice(index + q.length)}
    </span>
  );
}
