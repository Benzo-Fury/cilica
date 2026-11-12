/** Settings as grouped rows with a description under each label (bg-raised groups). */
import type { ReactNode } from "react";
import { cn } from "./cn";

export function SettingsPage({
  title,
  description,
  children,
}: {
  title: string;
  description?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="nt-settings-page">
      <header className="nt-settings-head">
        <h1 className="t-title">{title}</h1>
        {description && <p className="t-callout">{description}</p>}
      </header>
      {children}
    </div>
  );
}

export function SettingsGroup({
  title,
  footer,
  children,
  className,
}: {
  title?: string;
  footer?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("nt-settings-group", className)}>
      {title && <h2 className="nt-settings-group-title">{title}</h2>}
      <div className="nt-card nt-settings-card">{children}</div>
      {footer && <p className="nt-settings-foot">{footer}</p>}
    </section>
  );
}

export function SettingsRow({
  label,
  description,
  control,
  htmlFor,
  id,
  children,
}: {
  label: ReactNode;
  description?: ReactNode;
  control?: ReactNode;
  htmlFor?: string;
  id?: string;
  /** Extra content under the row (a swatch grid, a list). */
  children?: ReactNode;
}) {
  return (
    <div className="nt-settings-row">
      <div className="nt-settings-row-main">
        <div className="nt-settings-text">
          {htmlFor ? (
            <label id={id} htmlFor={htmlFor} className="nt-settings-label">
              {label}
            </label>
          ) : (
            <div id={id} className="nt-settings-label">
              {label}
            </div>
          )}
          {description && <div className="nt-settings-desc">{description}</div>}
        </div>
        {control && <div className="nt-settings-control">{control}</div>}
      </div>
      {children}
    </div>
  );
}
