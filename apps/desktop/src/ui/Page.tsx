/** Page scaffolding inside the canvas: the canvas band and a centred column. */
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

/** Folder / title breadcrumb at the start of the canvas band. */
export function Crumb({ parts }: { parts: readonly ReactNode[] }) {
  return (
    <span className="nt-crumb">
      {parts.map((part, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: positional breadcrumb parts
        <span key={i} className={i === parts.length - 1 ? "nt-crumb-current" : undefined}>
          {i > 0 && <span className="nt-crumb-sep">/</span>}
          {i === parts.length - 1 ? <span className="nt-crumb-text">{part}</span> : part}
        </span>
      ))}
    </span>
  );
}

/** A centred column at the note measure (720px). */
export function PageColumn({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("nt-page", className)} {...props} />;
}

/** Horizontal group for band items with the standard 4px gap. */
export function Toolbar({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("nt-toolbar", className)} {...props} />;
}

/** A flexible spacer that is also a drag region. */
export function BandSpacer() {
  return <span className="nt-band-spacer" data-tauri-drag-region />;
}

/** A simple list row for pages (Trash, history): title, meta and trailing actions. */
export function ListItem({
  heading,
  meta,
  actions,
  selected,
  className,
  ...props
}: HTMLAttributes<HTMLDivElement> & {
  heading: ReactNode;
  meta?: ReactNode;
  actions?: ReactNode;
  selected?: boolean;
}) {
  return (
    <div
      className={cn(
        "nt-list-item nt-anim-host nt-anim",
        selected && "nt-list-item-selected",
        className,
      )}
      {...props}
    >
      <div className="nt-list-text">
        <div className="nt-list-title">{heading}</div>
        {meta && <div className="nt-list-meta">{meta}</div>}
      </div>
      {actions && <div className="nt-list-actions">{actions}</div>}
    </div>
  );
}
