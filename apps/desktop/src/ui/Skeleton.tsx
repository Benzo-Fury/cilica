import { cn } from "./cn";

/** A shimmering bar that holds the shape of content while it loads (never for < 200ms). */
export function Skeleton({
  className,
  size = "line",
}: {
  className?: string;
  size?: "title" | "line";
}) {
  return (
    <span aria-hidden className={cn("nt-skel", size === "title" && "nt-skel-title", className)} />
  );
}

/** A paragraph of skeleton lines; the last one is shorter. */
export function SkeletonText({ lines = 3, className }: { lines?: number; className?: string }) {
  return (
    <span className={cn("nt-skel-text", className)} aria-hidden>
      {Array.from({ length: lines }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: static placeholder lines
        <Skeleton key={i} className={i === lines - 1 ? "nt-skel-short" : undefined} />
      ))}
    </span>
  );
}

/** Shimmer text for Margin's work in progress: say what is happening and how big it is. */
export function Shimmer({ children, className }: { children: string; className?: string }) {
  return <span className={cn("nt-shimmer", className)}>{children}</span>;
}
