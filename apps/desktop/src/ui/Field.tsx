import type { LucideIcon } from "lucide-react";
import type { InputHTMLAttributes, ReactNode, Ref } from "react";
import { cn } from "./cn";
import { Icon } from "./Icon";

export interface FieldProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "size"> {
  icon?: LucideIcon;
  /** Trailing content: a Kbd, a clear button. */
  trailing?: ReactNode;
  /** Error text under the field; also switches the field to its error state (shakes once). */
  error?: string | null;
  size?: "sm" | "md";
  fieldClassName?: string;
  ref?: Ref<HTMLInputElement>;
}

/** Short text: search, names, rename. Placeholders are examples, not instructions. */
export function Field({
  icon,
  trailing,
  error,
  size = "md",
  className,
  fieldClassName,
  id,
  ...props
}: FieldProps) {
  const errorId = error && id ? `${id}-error` : undefined;
  return (
    <div className={cn("nt-field-wrap", className)}>
      <label
        className={cn(
          "nt-field",
          size === "sm" && "nt-field-sm",
          error && "nt-field-error",
          fieldClassName,
        )}
      >
        {icon && <Icon icon={icon} size="sm" />}
        <input
          id={id}
          aria-invalid={error ? true : undefined}
          aria-describedby={errorId}
          spellCheck={false}
          autoComplete="off"
          {...props}
        />
        {trailing}
      </label>
      {error && (
        <p id={errorId} className="nt-field-msg">
          {error}
        </p>
      )}
    </div>
  );
}
