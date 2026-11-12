/**
 * Squircle corners (README › Shape). Where the WebView supports `corner-shape: squircle`
 * (Chromium 139+, WebView2) bundle.css does it in CSS. Elsewhere (WKWebView, WebKitGTK) the
 * large surfaces get an SVG clip-path from figma-squircle (cornerSmoothing 0.6); their shadow
 * stays on an unclipped parent.
 */
import { getSvgPath } from "figma-squircle";
import { type RefObject, useLayoutEffect } from "react";

export const supportsCornerShape: boolean =
  typeof CSS !== "undefined" && typeof CSS.supports === "function"
    ? CSS.supports("corner-shape", "squircle")
    : false;

/** Clip `ref` to a squircle of `radius` px while it is mounted (no-op where CSS can do it). */
export function useSquircleClip<T extends HTMLElement>(
  ref: RefObject<T | null>,
  radius: number,
  enabled = true,
) {
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !enabled || supportsCornerShape || typeof ResizeObserver === "undefined") return;
    const apply = () => {
      const { offsetWidth: width, offsetHeight: height } = el;
      if (!width || !height) return;
      const path = getSvgPath({
        width,
        height,
        cornerRadius: Math.min(radius, width / 2, height / 2),
        cornerSmoothing: 0.6,
      });
      el.style.clipPath = `path('${path}')`;
    };
    apply();
    const observer = new ResizeObserver(apply);
    observer.observe(el);
    return () => {
      observer.disconnect();
      el.style.clipPath = "";
    };
  }, [ref, radius, enabled]);
}
