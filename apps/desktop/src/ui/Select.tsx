/**
 * A compact choice from a list (input device, language, provider): a secondary button that opens
 * a strong-glass menu with a check on the current option.
 */
import { Check, ChevronsUpDown } from "lucide-react";
import { DropdownMenu as DM } from "radix-ui";
import type { CSSProperties } from "react";
import { cn } from "./cn";
import { Icon } from "./Icon";

export interface SelectOption<V extends string> {
  value: V;
  label: string;
  /** A second line under the label. */
  hint?: string;
}

export function SelectMenu<V extends string>({
  value,
  options,
  onChange,
  label,
  disabled,
  className,
}: {
  value: V;
  options: readonly SelectOption<V>[];
  onChange: (value: V) => void;
  /** Accessible name of the control. */
  label: string;
  disabled?: boolean;
  className?: string;
}) {
  const current = options.find((o) => o.value === value);
  return (
    <DM.Root>
      <DM.Trigger asChild disabled={disabled}>
        <button
          type="button"
          className={cn("nt-btn nt-btn-secondary nt-btn-sm nt-select", className)}
          aria-label={`${label}: ${current?.label ?? ""}`}
        >
          <span className="nt-select-label">{current?.label ?? "Choose"}</span>
          <Icon icon={ChevronsUpDown} size="sm" className="nt-faint" />
        </button>
      </DM.Trigger>
      <DM.Portal>
        <DM.Content
          align="end"
          sideOffset={6}
          collisionPadding={8}
          loop
          className="nt-menu nt-glass-strong nt-menu-layer"
          style={
            { "--origin": "var(--radix-dropdown-menu-content-transform-origin)" } as CSSProperties
          }
        >
          <DM.RadioGroup value={value} onValueChange={(v) => onChange(v as V)}>
            {options.map((o) => (
              <DM.RadioItem key={o.value} value={o.value} className="nt-menu-item nt-select-item">
                <span className="nt-select-check">
                  <DM.ItemIndicator>
                    <Icon icon={Check} size="sm" />
                  </DM.ItemIndicator>
                </span>
                <span className="nt-menu-label">
                  {o.label}
                  {o.hint && <span className="nt-select-hint">{o.hint}</span>}
                </span>
              </DM.RadioItem>
            ))}
          </DM.RadioGroup>
        </DM.Content>
      </DM.Portal>
    </DM.Root>
  );
}
