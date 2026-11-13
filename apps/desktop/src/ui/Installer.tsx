/**
 * The Windows installer window's parts (docs/design-system/components/InstallerWindow): a frameless
 * 600 × 480 window over the setup ridgeline, Cilica's own title band, one screen at a time, the
 * options panel that opens under Install, and the progress list. Presentational only; the
 * installer feature owns the flow and talks to Cilica Setup.
 */
import { AlertTriangle, Check } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { Progress } from "./Progress";

// ───────── Frame ─────────

export function InstallerFrame({
  ready,
  art,
  band,
  children,
}: {
  /** Fades and lifts the art in once the window has painted. */
  ready: boolean;
  art: ReactNode;
  band: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="nt-installer" data-ready={ready || undefined}>
      <div className="nt-installer-art" aria-hidden>
        {art}
        <div className="nt-installer-veil" />
      </div>
      {band}
      <main className="nt-installer-body">{children}</main>
    </div>
  );
}

/** The 44px band: the app icon and window name, then the window's own buttons. Drags the window. */
export function InstallerBand({
  icon,
  title,
  end,
}: {
  icon: ReactNode;
  title: string;
  end: ReactNode;
}) {
  return (
    <div className="nt-installer-band" data-tauri-drag-region>
      <span className="nt-installer-icon" aria-hidden>
        {icon}
      </span>
      <span className="nt-installer-title" data-tauri-drag-region>
        {title}
      </span>
      {end}
    </div>
  );
}

// ───────── Screens ─────────

/** One screen: a Newsreader heading, a lead, then whatever the screen holds. Lifts in on change. */
export function InstallerScreen({
  title,
  lead,
  children,
}: {
  title: ReactNode;
  lead?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <section className="nt-installer-screen">
      <h1>{title}</h1>
      {lead && <p className="nt-installer-lead">{lead}</p>}
      {children}
    </section>
  );
}

/** Pushes what follows to the bottom of the screen. */
export function InstallerSpacer() {
  return <span className="nt-installer-spacer" />;
}

export function InstallerActions({ children }: { children: ReactNode }) {
  return <div className="nt-installer-actions">{children}</div>;
}

export function InstallerHint({ children }: { children: ReactNode }) {
  return <span className="nt-installer-hint">{children}</span>;
}

/** A text button inside a hint ("Options"). */
export function InstallerLink({
  children,
  expanded,
  controls,
  onClick,
}: {
  children: ReactNode;
  expanded?: boolean;
  controls?: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="nt-installer-link"
      aria-expanded={expanded}
      aria-controls={controls}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

/** What went wrong and what to do, under the heading. */
export function InstallerProblem({ children }: { children: ReactNode }) {
  return (
    <p className="nt-installer-problem" role="alert">
      <Icon icon={AlertTriangle} size="sm" />
      <span>{children}</span>
    </p>
  );
}

/** A path or folder, set in the mono face and cut short with an ellipsis. */
export function InstallerPath({ children }: { children: string }) {
  return (
    <span className="nt-installer-path" title={children}>
      {children}
    </span>
  );
}

// ───────── Options ─────────

/** A glass panel of rows. With `open`, it opens and closes on the smooth spring. */
export function InstallerOptions({
  id,
  open = true,
  label,
  children,
}: {
  id?: string;
  open?: boolean;
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="nt-installer-options" id={id} data-open={open || undefined}>
      <div className="nt-installer-options-inner" inert={!open || undefined}>
        {/* biome-ignore lint/a11y/useSemanticElements: a group of mixed controls, not a form field set */}
        <div className="nt-installer-panel" role="group" aria-label={label}>
          {children}
        </div>
      </div>
    </div>
  );
}

/** One row: its label and a quieter line under it, then its control. */
export function InstallerOption({
  id,
  label,
  detail,
  control,
}: {
  id: string;
  label: string;
  detail?: ReactNode;
  control: ReactNode;
}) {
  return (
    <div className="nt-installer-option">
      <span className="nt-installer-option-text">
        <span className="nt-installer-option-label" id={id}>
          {label}
        </span>
        {detail && <span className="nt-installer-option-detail">{detail}</span>}
      </span>
      {control}
    </div>
  );
}

// ───────── Progress ─────────

export interface InstallerDoneLine {
  key: string;
  text: string;
}

/** What is happening now, the bar with its percentage, and a checked line per finished step. */
export function InstallerProgress({
  status,
  value,
  label,
  done,
  failed,
}: {
  status: string;
  /** 0..1 */
  value: number;
  label: string;
  done: readonly InstallerDoneLine[];
  failed?: boolean;
}) {
  return (
    <div className="nt-installer-progress" data-failed={failed || undefined}>
      <div className="nt-installer-status">
        <span aria-live="polite">{status}</span>
        <span className="nt-installer-percent">{Math.round(value * 100)}%</span>
      </div>
      <Progress value={value} label={label} />
      <ul className="nt-installer-done">
        {done.map((line) => (
          <li key={line.key}>
            <Icon icon={Check} size="sm" />
            {line.text}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** The band's app icon: Cilica's own tile. */
export function InstallerAppIcon({ src, className }: { src: string; className?: string }) {
  return <img className={cn("nt-installer-appicon", className)} src={src} alt="" />;
}
