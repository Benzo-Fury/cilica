import { cn } from "./cn";

export interface KbdProps {
  /** Keycaps, already resolved for the platform (`useShortcut(id)` gives these). */
  keys: readonly string[];
  /** On an accent fill (primary buttons) the caps take the button's colour. */
  tone?: "default" | "on-accent";
  className?: string;
}

/** A keyboard shortcut as small keycaps. Hints, not content: always `ink-3`. */
export function Kbd({ keys, tone = "default", className }: KbdProps) {
  if (keys.length === 0) return null;
  return (
    <span className={cn("nt-kbd-group", tone === "on-accent" && "nt-kbd-on-accent", className)}>
      {keys.map((k, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: keycaps can repeat and never reorder
        <kbd key={`${k}-${i}`} className="nt-kbd">
          {k}
        </kbd>
      ))}
    </span>
  );
}
