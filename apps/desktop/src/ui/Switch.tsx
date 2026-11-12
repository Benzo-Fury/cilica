import type { ButtonHTMLAttributes } from "react";
import { cn } from "./cn";

export interface SwitchProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onChange" | "role"> {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  /** The accessible name when the visible label is not linked with `aria-labelledby`. */
  label?: string;
}

/** Turns a setting on or off immediately. The knob springs and squishes while pressed. */
export function Switch({ checked, onCheckedChange, label, className, ...props }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={cn("nt-switch", className)}
      onClick={() => onCheckedChange(!checked)}
      {...props}
    />
  );
}
