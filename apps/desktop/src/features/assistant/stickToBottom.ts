/**
 * Keep a conversation pinned to its newest words while they stream in, until you scroll up.
 *
 * Following stops only when you move up: content growing below you, or a smooth scroll still on
 * its way down, never counts as leaving. Scrolling back near the bottom picks it up again. Size
 * changes (the floating bar springing taller, a card expanding) keep it pinned too.
 */
import { useCallback, useEffect, useRef } from "react";

/** How close to the bottom still counts as "at the bottom", in pixels. */
export const NEAR_BOTTOM = 48;

/** Whether to keep following after a scroll moved the view from `previousTop` to `top`. */
export function nextFollowing(
  following: boolean,
  { top, previousTop, distance }: { top: number; previousTop: number; distance: number },
): boolean {
  if (distance < NEAR_BOTTOM) return true;
  if (top < previousTop) return false;
  return following;
}

/**
 * Returns a callback ref for the scrolling element and its scroll handler. `changeKey` changes
 * whenever new content arrives (a message, a streamed chunk).
 */
export function useStickToBottom(changeKey: unknown) {
  const elRef = useRef<HTMLElement | null>(null);
  const following = useRef(true);
  const lastTop = useRef(0);
  const observer = useRef<ResizeObserver | null>(null);

  const pin = useCallback((smooth: boolean) => {
    const el = elRef.current;
    if (!el || !following.current) return;
    el.scrollTo({ top: el.scrollHeight, behavior: smooth ? "smooth" : "auto" });
  }, []);

  // Watch the element and its content, so growth that happens after a render still pins
  const ref = useCallback(
    (el: HTMLElement | null) => {
      observer.current?.disconnect();
      elRef.current = el;
      if (!el) return;
      following.current = true;
      lastTop.current = el.scrollTop;
      const resize = new ResizeObserver(() => pin(false));
      resize.observe(el);
      for (const child of Array.from(el.children)) resize.observe(child);
      observer.current = resize;
      pin(false);
    },
    [pin],
  );
  useEffect(() => () => observer.current?.disconnect(), []);

  // New words: glide down to them
  // biome-ignore lint/correctness/useExhaustiveDependencies: follow every change of the key
  useEffect(() => pin(true), [changeKey, pin]);

  const onScroll = useCallback(() => {
    const el = elRef.current;
    if (!el) return;
    following.current = nextFollowing(following.current, {
      top: el.scrollTop,
      previousTop: lastTop.current,
      distance: el.scrollHeight - el.clientHeight - el.scrollTop,
    });
    lastTop.current = el.scrollTop;
  }, []);

  return { ref, onScroll };
}
