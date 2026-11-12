import { type PointerEvent, useCallback } from "react";

/** Feeds the pointer position to `.nt-specular` / `.nt-btn-glass` (`--mx`, `--my`). */
export function useSpecular<T extends HTMLElement>() {
  return useCallback((event: PointerEvent<T>) => {
    const el = event.currentTarget;
    const rect = el.getBoundingClientRect();
    el.style.setProperty("--mx", `${event.clientX - rect.left}px`);
    el.style.setProperty("--my", `${event.clientY - rect.top}px`);
  }, []);
}
