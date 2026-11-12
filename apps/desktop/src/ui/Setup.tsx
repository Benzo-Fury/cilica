/**
 * The first-run setup window's parts (docs/design-system/components/SetupWindow): the frame with
 * its background art and title band, the welcome screen, steps that open one at a time and
 * collapse to a line when done, permission buttons, choice cards, app tiles and the pills that
 * report downloads and imports still running. Presentational only; the onboarding feature owns
 * the flow and talks to the backend.
 */
import { ArrowRight, Check, ChevronRight, type LucideIcon } from "lucide-react";
import type { CSSProperties, ReactNode } from "react";
import { cn } from "./cn";
import { Icon } from "./Icon";

// ───────── Frame ─────────

export interface SetupFrameProps {
  /** "welcome" hides the band's title and step count. */
  phase: "welcome" | "steps";
  /** Fades and lifts the art in once the window has painted. */
  ready: boolean;
  art: ReactNode;
  band: ReactNode;
  /** Pills for work that keeps going in the background. */
  tray?: ReactNode;
  children: ReactNode;
}

export function SetupFrame({ phase, ready, art, band, tray, children }: SetupFrameProps) {
  return (
    <div className="nt-setup" data-phase={phase} data-ready={ready || undefined}>
      <div className="nt-setup-art" aria-hidden>
        {art}
        <div className="nt-setup-veil-blur" />
        <div className="nt-setup-veil" />
      </div>
      {band}
      {children}
      <div className="nt-setup-tray" aria-live="polite">
        {tray}
      </div>
    </div>
  );
}

/** The 44px title band: window controls at the start, a centred title, the count and extras at the end. */
export function SetupBand({
  start,
  title,
  count,
  end,
}: {
  start: ReactNode;
  title: string;
  count?: string;
  end?: ReactNode;
}) {
  return (
    <div className="nt-setup-band" data-tauri-drag-region>
      {start}
      <span className="nt-setup-band-title" aria-hidden>
        {title}
      </span>
      <span className="nt-setup-band-end">
        {count && <span className="nt-setup-count">{count}</span>}
        {end}
      </span>
    </div>
  );
}

// ───────── Welcome ─────────

export interface SetupFeature {
  icon: LucideIcon;
  title: string;
  text: string;
}

export function SetupWelcome({
  title,
  lead,
  features,
  action,
  note,
  leaving,
}: {
  title: ReactNode;
  lead: string;
  features: readonly SetupFeature[];
  action: ReactNode;
  note?: string;
  /** Fades and lifts away before the steps take over. */
  leaving?: boolean;
}) {
  return (
    <div className="nt-setup-welcome" data-leaving={leaving || undefined}>
      <h1>{title}</h1>
      <p className="nt-setup-lead">{lead}</p>
      <ul className="nt-setup-features">
        {features.map((f) => (
          <li key={f.title}>
            <Icon icon={f.icon} size="sm" />
            <span>
              <span className="nt-setup-feature-title">{f.title}</span>
              <span className="nt-setup-feature-text">{f.text}</span>
            </span>
          </li>
        ))}
      </ul>
      <div className="nt-setup-start">
        {action}
        {note && <span className="nt-setup-note">{note}</span>}
      </div>
    </div>
  );
}

// ───────── Steps ─────────

export function SetupSteps({
  children,
  ref,
}: {
  children: ReactNode;
  ref?: React.Ref<HTMLDivElement>;
}) {
  return (
    <div className="nt-setup-steps" ref={ref}>
      {children}
    </div>
  );
}

export type SetupStepStatus = "done" | "active";

export interface SetupStepProps {
  id: string;
  status: SetupStepStatus;
  /** The heading while open. */
  title: string;
  /** The one line shown once done ("Microphone and call audio on"). */
  summary: string;
  /** Done by skipping: the check turns grey. */
  skipped?: boolean;
  /** Reopen a finished step from its line. */
  onReopen?: () => void;
  /** Shown beside the open step's title. */
  skipLabel?: string;
  onSkip?: () => void;
  description?: ReactNode;
  children?: ReactNode;
  actions?: ReactNode;
}

/** A step of the setup column. Its body opens on the smooth spring and collapses when done. */
export function SetupStep({
  id,
  status,
  title,
  summary,
  skipped,
  onReopen,
  skipLabel = "Skip",
  onSkip,
  description,
  children,
  actions,
}: SetupStepProps) {
  const done = status === "done";
  const headingId = `setup-step-${id}`;
  return (
    <section
      className="nt-setup-step"
      data-status={status}
      data-skipped={(done && skipped) || undefined}
      data-step={id}
      aria-labelledby={headingId}
    >
      {done ? (
        <button
          type="button"
          className="nt-setup-step-head"
          onClick={onReopen}
          aria-label={`${summary}. Change`}
        >
          <span className="nt-setup-step-check">
            <Icon icon={Check} size="sm" />
          </span>
          <h2 id={headingId} className="nt-setup-step-title">
            {summary}
          </h2>
        </button>
      ) : (
        <div className="nt-setup-step-head">
          <h2 id={headingId} className="nt-setup-step-title">
            {title}
          </h2>
          {onSkip && (
            <button type="button" className="nt-setup-skip" onClick={onSkip}>
              {skipLabel}
              <Icon icon={ChevronRight} size="sm" />
            </button>
          )}
        </div>
      )}
      <div className="nt-setup-step-body" inert={done || undefined}>
        <div className="nt-setup-step-inner">
          <div className="nt-setup-step-pad">
            {description && <p className="nt-setup-desc">{description}</p>}
            {children}
            {actions && <div className="nt-setup-actions">{actions}</div>}
          </div>
        </div>
      </div>
    </section>
  );
}

// ───────── Controls inside steps ─────────

export function SetupPermissions({ children }: { children: ReactNode }) {
  return <div className="nt-setup-perms">{children}</div>;
}

export interface SetupPermissionProps {
  icon: LucideIcon;
  /** The invitation ("Let Margin hear you") and the confirmation ("Margin can hear you"). */
  title: string;
  grantedTitle: string;
  description: string;
  grantedDescription: string;
  granted: boolean;
  /** The one next permission to grant gets the accent. */
  primary?: boolean;
  busy?: boolean;
  optional?: boolean;
  onClick: () => void;
}

export function SetupPermission({
  icon,
  title,
  grantedTitle,
  description,
  grantedDescription,
  granted,
  primary,
  busy,
  optional,
  onClick,
}: SetupPermissionProps) {
  return (
    <button
      type="button"
      className={cn("nt-setup-perm", !granted && !primary && "nt-setup-glass")}
      data-primary={(primary && !granted) || undefined}
      data-granted={granted || undefined}
      aria-busy={busy || undefined}
      aria-disabled={granted || undefined}
      onClick={granted || busy ? undefined : onClick}
    >
      <span className="nt-setup-perm-icon">
        <Icon icon={granted ? Check : icon} size="sm" />
      </span>
      <span className="nt-setup-perm-text">
        <span className="nt-setup-perm-title">
          {granted ? grantedTitle : title}
          {optional && !granted && <span className="nt-setup-tag">Optional</span>}
        </span>
        <span className="nt-setup-perm-sub">{granted ? grantedDescription : description}</span>
      </span>
      {!granted && <Icon icon={ArrowRight} size="sm" className="nt-setup-perm-go" />}
    </button>
  );
}

export function SetupPanel({ children }: { children: ReactNode }) {
  return <div className="nt-setup-panel nt-setup-glass">{children}</div>;
}

export function SetupPanelLine({ children }: { children: ReactNode }) {
  return <div className="nt-setup-panel-line">{children}</div>;
}

/** Title and a line under it, for panels and permission-like rows. */
export function SetupText({
  title,
  text,
  id,
}: {
  title: ReactNode;
  text?: ReactNode;
  id?: string;
}) {
  return (
    <span className="nt-setup-perm-text">
      <span className="nt-setup-perm-title" id={id}>
        {title}
      </span>
      {text && <span className="nt-setup-perm-sub">{text}</span>}
    </span>
  );
}

/** A stack of cards to pick from (speech models). */
export function SetupList({ children }: { children: ReactNode }) {
  return <div className="nt-setup-list">{children}</div>;
}

export function SetupCard({
  title,
  badge,
  description,
  progress,
  action,
}: {
  title: ReactNode;
  badge?: ReactNode;
  description: ReactNode;
  /** A progress bar under the text while something downloads. */
  progress?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="nt-setup-card">
      <div className="nt-setup-card-text">
        <div className="nt-setup-card-title">
          {title}
          {badge}
        </div>
        <div className="nt-setup-card-sub">{description}</div>
        {progress}
      </div>
      {action}
    </div>
  );
}

export function SetupChoices({
  label,
  columns = 2,
  children,
}: {
  label: string;
  columns?: 2 | 3;
  children: ReactNode;
}) {
  return (
    <div className="nt-setup-choices" role="radiogroup" aria-label={label} data-columns={columns}>
      {children}
    </div>
  );
}

export function SetupChoice({
  title,
  description,
  badge,
  preview,
  font,
  checked,
  onSelect,
}: {
  title: string;
  description?: ReactNode;
  badge?: ReactNode;
  /** Large sample above the title ("Aa" in the typeface). */
  preview?: ReactNode;
  /** Set the preview and description in one of the note typefaces. */
  font?: "sans" | "serif" | "mono";
  checked: boolean;
  onSelect: () => void;
}) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: a styled button keeps the design-system look and focus ring
    <button
      type="button"
      role="radio"
      aria-checked={checked}
      className="nt-setup-choice nt-setup-glass"
      data-font={font}
      onClick={onSelect}
    >
      {preview && <span className="nt-setup-choice-preview">{preview}</span>}
      <span className="nt-setup-choice-title">
        {title}
        {badge}
      </span>
      {description && <span className="nt-setup-choice-sub">{description}</span>}
    </button>
  );
}

export function SetupApps({ children }: { children: ReactNode }) {
  return <div className="nt-setup-apps">{children}</div>;
}

export function SetupApp({
  logo,
  name,
  detail,
  selected,
  disabled,
  onToggle,
}: {
  logo?: string | undefined;
  name: string;
  detail: string;
  selected: boolean;
  disabled?: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      className="nt-setup-app nt-setup-glass"
      aria-pressed={selected}
      disabled={disabled}
      onClick={onToggle}
    >
      <span className="nt-setup-app-tick" aria-hidden>
        <Icon icon={Check} size="sm" />
      </span>
      {logo && <img src={logo} alt="" />}
      <span className="nt-setup-app-name">{name}</span>
      <span className="nt-setup-app-sub">{detail}</span>
    </button>
  );
}

/** A labelled field: the label stays above it, so the placeholder is only an example. */
export function SetupField({
  label,
  htmlFor,
  children,
}: {
  label: string;
  htmlFor: string;
  children: ReactNode;
}) {
  return (
    <div className="nt-setup-field">
      <label className="nt-setup-label" htmlFor={htmlFor}>
        {label}
      </label>
      {children}
    </div>
  );
}

export function SetupFields({ children }: { children: ReactNode }) {
  return <div className="nt-setup-fields">{children}</div>;
}

/** A field with its button beside it (Save key, Connect). */
export function SetupFieldRow({ children }: { children: ReactNode }) {
  return <div className="nt-setup-field-row">{children}</div>;
}

export function SetupNote({ children }: { children: ReactNode }) {
  return <p className="nt-setup-note">{children}</p>;
}

export function SetupWarning({ icon, children }: { icon: LucideIcon; children: ReactNode }) {
  return (
    <p className="nt-setup-warning">
      <Icon icon={icon} size="sm" />
      <span>{children}</span>
    </p>
  );
}

// ───────── Background work ─────────

/** A download or import that keeps going while you move on. `value` is 0..1, null while unknown. */
export function SetupTask({ label, value }: { label: string; value: number | null }) {
  return (
    <div className="nt-setup-pill">
      <span
        className="nt-setup-ring"
        data-unknown={value === null || undefined}
        style={{ "--p": value ?? 0.3 } as CSSProperties}
      />
      <span>{label}</span>
    </div>
  );
}
