import type { ButtonHTMLAttributes } from "react";
import { cn } from "./cn";

export interface CheckboxProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onChange" | "role"> {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  /** Required when there is no visible label next to it. */
  label?: string;
}

/** Fill fades to `accent` and the tick draws itself in. */
export function Checkbox({ checked, onCheckedChange, label, className, ...props }: CheckboxProps) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: a styled button keeps the design-system look and focus ring
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      aria-label={label}
      className={cn("nt-check", className)}
      onClick={() => onCheckedChange(!checked)}
      {...props}
    >
      <svg
        viewBox="0 0 12 12"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        <path d="M2.5 6.5l2.3 2.3 4.7-5" />
      </svg>
    </button>
  );
}
