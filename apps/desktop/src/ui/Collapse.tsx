import type { ReactNode } from "react";
import { cn } from "./cn";

/**
 * Opens and closes its children by animating `grid-template-rows` (0fr ↔ 1fr) with
 * `--spring-smooth` plus a `--dur-base` fade, as bundle.css's `.nt-collapse`.
 */
export function Collapse({
  open,
  children,
  className,
  id,
}: {
  open: boolean;
  children: ReactNode;
  className?: string;
  id?: string;
}) {
  return (
    <div
      id={id}
      className={cn("nt-collapse", className)}
      data-closed={open ? undefined : ""}
      inert={!open}
    >
      <div>{children}</div>
    </div>
  );
}
