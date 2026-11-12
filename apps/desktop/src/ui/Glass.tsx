import type { HTMLAttributes, Ref } from "react";
import { cn } from "./cn";
import { useSpecular } from "./useSpecular";

export interface GlassProps extends HTMLAttributes<HTMLDivElement> {
  /** `regular` for floating controls; `strong` for anything with text to read. */
  strength?: "regular" | "strong";
  /** A soft highlight that follows the pointer. */
  specular?: boolean;
  ref?: Ref<HTMLDivElement>;
}

/** In-app frosted glass for controls that float over the canvas. Never stack glass on glass. */
export function Glass({
  strength = "regular",
  specular = false,
  className,
  onPointerMove,
  ...props
}: GlassProps) {
  const track = useSpecular<HTMLDivElement>();
  return (
    <div
      className={cn(
        strength === "strong" ? "nt-glass-strong" : "nt-glass",
        specular && "nt-specular",
        className,
      )}
      onPointerMove={(e) => {
        if (specular) track(e);
        onPointerMove?.(e);
      }}
      {...props}
    />
  );
}
