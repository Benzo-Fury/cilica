/**
 * Pieces for Settings › Import: an app's mark, a searchable picker of apps, numbered steps with
 * the things to click set apart, and a drop zone.
 */
import { Command } from "cmdk";
import { AppWindow, ChevronsUpDown, FolderUp, type LucideIcon, Search } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";
import { type ReactNode, useState } from "react";
import { springs } from "@/app/motion";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { PaletteGroup, PaletteItem } from "./Palette";
import { Popover, PopoverContent, PopoverTrigger } from "./Popover";

// ── An app's mark ─────────────────────────────────────────────────────────

/** An app's own icon, a symbol for things that are not an app, or else its first letter. */
export function ProviderIcon({
  name,
  logo,
  glyph,
  size = "md",
  className,
}: {
  name: string;
  /** Image URL of the app's icon. */
  logo?: string;
  glyph?: LucideIcon;
  size?: "sm" | "md" | "lg";
  className?: string;
}) {
  return (
    <span
      className={cn("nt-provider-icon", `nt-provider-icon-${size}`, logo && "has-logo", className)}
      aria-hidden
    >
      {logo ? (
        <img src={logo} alt="" draggable={false} decoding="async" />
      ) : glyph ? (
        <Icon icon={glyph} size={size === "lg" ? "md" : "sm"} />
      ) : (
        <span className="nt-provider-mono">{name.trim().charAt(0).toUpperCase()}</span>
      )}
    </span>
  );
}

// ── Picker ───────────────────────────────────────────────────────────────

export interface IconPickerOption {
  value: string;
  label: string;
  hint?: string;
  icon: ReactNode;
  /** A short mark on the right ("Found"). */
  badge?: ReactNode;
  keywords?: string[];
}

export interface IconPickerGroup {
  heading: string;
  options: IconPickerOption[];
}

/** A large select with icons, grouped and searchable. */
export function IconPicker({
  value,
  groups,
  onChange,
  label,
  placeholder,
  searchPlaceholder,
}: {
  value: string | null;
  groups: IconPickerGroup[];
  onChange: (value: string) => void;
  /** Accessible name of the control. */
  label: string;
  placeholder: string;
  searchPlaceholder: string;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [highlighted, setHighlighted] = useState(value ?? "");
  const current = groups.flatMap((g) => g.options).find((o) => o.value === value);
  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (next) {
          setQuery("");
          setHighlighted(value ?? "");
        }
      }}
    >
      <PopoverTrigger asChild>
        <button
          type="button"
          className="nt-picker"
          aria-label={`${label}: ${current?.label ?? placeholder}`}
        >
          {current ? (
            current.icon
          ) : (
            <span className="nt-provider-icon" aria-hidden>
              <Icon icon={AppWindow} size="sm" className="nt-faint" />
            </span>
          )}
          <span className="nt-picker-text">
            <span className={cn("nt-picker-label", !current && "nt-faint")}>
              {current?.label ?? placeholder}
            </span>
            {current?.hint && <span className="nt-picker-hint">{current.hint}</span>}
          </span>
          {current?.badge}
          <Icon icon={ChevronsUpDown} size="sm" className="nt-faint" />
        </button>
      </PopoverTrigger>
      <PopoverContent label={label} className="nt-picker-menu nt-palette">
        <Command label={label} loop value={highlighted} onValueChange={setHighlighted}>
          <div className="nt-palette-input nt-picker-search">
            <Icon icon={Search} />
            <Command.Input
              value={query}
              onValueChange={setQuery}
              placeholder={searchPlaceholder}
              autoFocus
            />
          </div>
          <Command.List className="nt-palette-list nt-palette-scroll">
            <Command.Empty className="nt-palette-empty">
              No app by that name. Try “Other files”.
            </Command.Empty>
            {groups.map((g) => (
              <PaletteGroup key={g.heading} heading={g.heading}>
                {g.options.map((o) => (
                  <PaletteItem
                    key={o.value}
                    value={o.value}
                    keywords={[o.label, ...(o.keywords ?? [])]}
                    selected={highlighted === o.value}
                    leading={o.icon}
                    hint={o.badge}
                    onSelect={() => {
                      onChange(o.value);
                      setOpen(false);
                    }}
                  >
                    {o.label}
                  </PaletteItem>
                ))}
              </PaletteGroup>
            ))}
          </Command.List>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

// ── Steps ────────────────────────────────────────────────────────────────

/**
 * Inline markup for instructions: `**Settings › API**` is something to click (each › part set
 * apart), `` `takeout.google.com` `` is something to type or a file name.
 */
export function RichText({ text }: { text: string }) {
  const parts = text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g).filter(Boolean);
  return (
    <>
      {parts.map((part, i) => {
        const key = `${i}-${part}`;
        if (part.startsWith("**") && part.endsWith("**")) {
          const path = part
            .slice(2, -2)
            .split("›")
            .map((p) => p.trim());
          return (
            <span key={key} className="nt-ui-path">
              {path.map((p, j) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: a menu path's parts are fixed text in a fixed order
                <span key={`${j}-${p}`} className="nt-ui-path-part">
                  {j > 0 && (
                    <span className="nt-ui-path-sep" aria-hidden>
                      ›
                    </span>
                  )}
                  <b>{p}</b>
                </span>
              ))}
            </span>
          );
        }
        if (part.startsWith("`") && part.endsWith("`")) {
          return (
            <code key={key} className="nt-inline-code">
              {part.slice(1, -1)}
            </code>
          );
        }
        return <span key={key}>{part}</span>;
      })}
    </>
  );
}

export function StepList({ steps, className }: { steps: readonly string[]; className?: string }) {
  return (
    <ol className={cn("nt-steps", className)}>
      {steps.map((step, i) => (
        <li key={step}>
          <span className="nt-step-n" aria-hidden>
            {i + 1}
          </span>
          <span className="nt-step-text">
            <RichText text={step} />
          </span>
        </li>
      ))}
    </ol>
  );
}

// ── Drop zone ────────────────────────────────────────────────────────────

/** Where files land. `active` while something is dragged over the window. */
export function DropZone({
  active,
  title,
  hint,
  children,
}: {
  active: boolean;
  title: string;
  hint?: ReactNode;
  children?: ReactNode;
}) {
  const reduced = useReducedMotion();
  return (
    <motion.div
      className={cn("nt-dropzone", active && "is-active")}
      animate={{ scale: active && !reduced ? 1.01 : 1 }}
      transition={springs.snappy}
      data-active={active || undefined}
    >
      <Icon icon={FolderUp} className="nt-dropzone-icon nt-i-up" />
      <div className="nt-dropzone-title">{active ? "Drop to import" : title}</div>
      {hint && <div className="nt-dropzone-hint">{hint}</div>}
      {children && <div className="nt-dropzone-actions">{children}</div>}
    </motion.div>
  );
}

/** A label and a value in a compact grid (counts in the preview). */
export function StatGrid({ items }: { items: readonly { label: string; value: ReactNode }[] }) {
  return (
    <dl className="nt-stats">
      {items.map((item) => (
        <div key={item.label} className="nt-stat">
          <dt>{item.label}</dt>
          <dd>{item.value}</dd>
        </div>
      ))}
    </dl>
  );
}
