import { cn } from "./cn";

/** A round colour swatch (speaker colours). The selected one gets the focus-style ring. */
export function Swatch({
  color,
  label,
  selected,
  onSelect,
}: {
  /** A CSS colour, such as `var(--speaker-*)`. */
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
