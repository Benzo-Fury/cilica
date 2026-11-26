/**
 * What an image node's `src` means for display:
 * - `attachments/…` → its file, through the asset protocol (or the mock's memory);
 * - `blob:` (just pasted, still saving) and `data:` → shown as they are;
 * - `https://…` → not loaded: the app never fetches web images on its own, so sites can't see
 *   when a note is opened. The card offers to save a copy;
 * - anything else (an imported vault's `images/pic.png`) → missing.
 */
import { useEffect, useState } from "react";
import { attachmentUrl, attachmentUrlNow, isAttachmentPath } from "@/ipc/attachments";

export type ImageSource =
  | { kind: "ready"; url: string }
  | { kind: "resolving" }
  | { kind: "web"; url: string }
  | { kind: "missing" };

export function classify(src: string | null): ImageSource {
  if (!src) return { kind: "missing" };
  if (src.startsWith("blob:") || src.startsWith("data:image/")) return { kind: "ready", url: src };
  if (/^https?:\/\//i.test(src)) return { kind: "web", url: src };
  if (isAttachmentPath(src)) {
    const now = attachmentUrlNow(src);
    return now ? { kind: "ready", url: now } : { kind: "resolving" };
  }
  return { kind: "missing" };
}

export function useImageSrc(src: string | null): ImageSource {
  const [source, setSource] = useState<ImageSource>(() => classify(src));
  useEffect(() => {
    const first = classify(src);
    setSource(first);
    if (first.kind !== "resolving" || !src) return;
    let live = true;
    attachmentUrl(src).then(
      (url) => live && setSource(url ? { kind: "ready", url } : { kind: "missing" }),
      () => live && setSource({ kind: "missing" }),
    );
    return () => {
      live = false;
    };
  }, [src]);
  return source;
}
