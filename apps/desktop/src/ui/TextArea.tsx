import type { Ref, TextareaHTMLAttributes } from "react";
import { cn } from "./cn";

/** Multi-line text (a task's prompt). Same ground and focus halo as Field. */
export function TextArea({
  className,
  ref,
  ...props
}: TextareaHTMLAttributes<HTMLTextAreaElement> & { ref?: Ref<HTMLTextAreaElement> }) {
  return <textarea ref={ref} className={cn("nt-textarea", className)} {...props} />;
}

/** A label above a control, for forms in dialogs. */
export function FormField({
  label,
  htmlFor,
  hint,
  children,
}: {
  label: string;
  htmlFor?: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="nt-form-field">
      <label className="nt-form-label" htmlFor={htmlFor}>
        {label}
      </label>
      {children}
      {hint && <p className="nt-field-msg">{hint}</p>}
    </div>
  );
}
