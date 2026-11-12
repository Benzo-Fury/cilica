import { cn } from "./cn";

/** A round colour swatch (accent presets). The selected one gets the focus-style ring. */
export function Swatch({
  color,
  label,
  selected,
  onSelect,
}: {
  /** A CSS colour, normally `var(--preset-*)`. */
  color: string;
  label: string;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: a styled button keeps the design-system look and focus ring
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      aria-label={label}
      title={label}
      className={cn("nt-swatch")}
      style={{ background: color }}
      onClick={onSelect}
    />
  );
}
