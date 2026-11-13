import type { CSSProperties } from "react";
import { cn } from "./cn";

export interface AvatarProps {
  /** A name ("Priya Shah") or initials. */
  name: string;
  /** 1..8: the `speaker-N` colour. */
  color: number;
  size?: "xs" | "sm" | "md" | "lg";
  className?: string;
  /** Position in a stack, used for the hover fan-out. */
  index?: number;
  /** Nobody yet (a speaker Cilica is still working out): an empty dashed ring. */
  pending?: boolean;
}

export function initials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1) return (words[0] ?? "").slice(0, 2).toUpperCase();
  return `${(words[0] ?? "")[0] ?? ""}${(words[words.length - 1] ?? "")[0] ?? ""}`.toUpperCase();
}

/** Initials on a soft speaker colour with a 1.5px ring in the full colour. */
export function Avatar({ name, color, size = "md", className, index, pending }: AvatarProps) {
  const n = Math.min(8, Math.max(1, Math.round(color)));
  const style = {
    "--c": `var(--speaker-${n})`,
    "--s": `var(--speaker-${n}-soft)`,
    "--i": index,
  } as CSSProperties;
  return (
    <span
      className={cn(
        "nt-avatar",
        size !== "md" && `nt-avatar-${size}`,
        pending && "nt-avatar-pending",
        className,
      )}
      style={style}
      title={name}
      role="img"
      aria-label={name}
    >
      {pending ? null : initials(name)}
    </span>
  );
}

export interface AvatarStackProps {
  people: readonly { name: string; color: number }[];
  size?: AvatarProps["size"];
  max?: number;
}

/** Overlapping avatars that fan out 4px each on hover. */
export function AvatarStack({ people, size = "xs", max = 3 }: AvatarStackProps) {
  return (
    <span className="nt-avatar-stack">
      {people.slice(0, max).map((p, i) => (
        <Avatar key={p.name} name={p.name} color={p.color} size={size} index={i} />
      ))}
    </span>
  );
}
