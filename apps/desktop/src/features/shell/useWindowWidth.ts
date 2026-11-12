/** The window's inner width, kept current as it is resized. */
import { useSyncExternalStore } from "react";

const subscribe = (onChange: () => void) => {
  window.addEventListener("resize", onChange);
  return () => window.removeEventListener("resize", onChange);
};

export function useWindowWidth(): number {
  return useSyncExternalStore(subscribe, () => window.innerWidth);
}
