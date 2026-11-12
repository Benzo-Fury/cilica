/**
 * "Esc closes the topmost layer." Non-Radix layers (search, find bar, history, settings pages,
 * the rebind recorder) register here; Radix dialogs and menus close themselves first and mark
 * the event handled, so the global handler only reaches this stack when nothing else took it.
 */
import { useEffect, useRef } from "react";

interface Layer {
  id: number;
  close: () => void;
}

const stack: Layer[] = [];
let nextId = 1;

/** Close the most recently opened layer. Returns false when there is none. */
export function closeTopLayer(): boolean {
  const top = stack[stack.length - 1];
  if (!top) return false;
  top.close();
  return true;
}

export function hasOpenLayer(): boolean {
  return stack.length > 0;
}

/** Register a layer while `open` is true. The latest `close` is always used. */
export function useEscapeLayer(open: boolean, close: () => void) {
  const closeRef = useRef(close);
  closeRef.current = close;
  useEffect(() => {
    if (!open) return;
    const layer: Layer = { id: nextId++, close: () => closeRef.current() };
    stack.push(layer);
    return () => {
      const index = stack.findIndex((l) => l.id === layer.id);
      if (index >= 0) stack.splice(index, 1);
    };
  }, [open]);
}
