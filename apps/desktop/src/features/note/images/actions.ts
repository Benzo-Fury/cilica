/**
 * What can be done to an image in a note, shared by its toolbar and its context menu:
 * view, copy, save a copy elsewhere, show the file, replace, save a web image, resize, place,
 * caption and remove. Each acts on the image at a document position.
 */
import type { Editor } from "@tiptap/core";
import type { Node as PMNode } from "@tiptap/pm/model";
import { NodeSelection } from "@tiptap/pm/state";
import { os } from "@/app/platform";
import { attachmentFile, attachmentUrl, isAttachmentPath } from "@/ipc/attachments";
import { api, errorMessage, isTauri } from "@/ipc/client";
import { toast } from "@/lib/toast";
import type { ViewerImage } from "@/ui";
import type { ImageAlign, ImageAttrs } from "../editor/image";
import { findPending, newKey } from "../editor/imageInput";
import { pickImages, reportImageError, saveImageSource } from "./saver";
import { useImageUi } from "./store";
import { classify } from "./useImageSrc";

export const revealLabel =
  os === "macos" ? "Show in Finder" : os === "windows" ? "Show in File Explorer" : "Show in folder";

function imageAt(editor: Editor, pos: number): PMNode | null {
  const node = editor.state.doc.nodeAt(pos);
  return node?.type.name === "image" ? node : null;
}

/** A URL to show or copy for this image's source. */
async function displayUrl(src: string | null): Promise<string | null> {
  const source = classify(src);
  if (source.kind === "ready") return source.url;
  if (source.kind === "resolving" && src) return attachmentUrl(src);
  return null;
}

/** Every image in the note the viewer can show, in order, and where `pos` falls among them. */
async function viewerImages(editor: Editor, pos: number) {
  const found: { pos: number; node: PMNode }[] = [];
  editor.state.doc.descendants((node, at) => {
    if (node.type.name === "image") found.push({ pos: at, node });
  });
  const images: ViewerImage[] = [];
  let index = 0;
  for (const item of found) {
    const url = await displayUrl(item.node.attrs.src);
    if (!url) continue;
    if (item.pos === pos) index = images.length;
    images.push({ src: url, alt: item.node.attrs.alt ?? "", caption: item.node.attrs.title ?? "" });
  }
  return { images, index };
}

/** PNG bytes for the clipboard (the one image type every system clipboard takes from a webview). */
async function asPng(blob: Blob): Promise<Blob> {
  if (blob.type === "image/png") return blob;
  const bitmap = await createImageBitmap(blob);
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  canvas.getContext("2d")?.drawImage(bitmap, 0, 0);
  bitmap.close();
  return new Promise((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("encode"))), "image/png"),
  );
}

const extensionOf = (src: string) => src.split(".").pop() ?? "png";

export function imageActions(editor: Editor) {
  const update = (pos: number, attrs: Partial<ImageAttrs>) => {
    const node = imageAt(editor, pos);
    if (!node) return;
    const tr = editor.state.tr.setNodeMarkup(pos, undefined, { ...node.attrs, ...attrs });
    tr.setSelection(NodeSelection.create(tr.doc, pos));
    editor.view.dispatch(tr);
  };

  return {
    update,

    select(pos: number) {
      if (!imageAt(editor, pos)) return;
      editor.view.dispatch(
        editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, pos)),
      );
      editor.view.focus();
    },

    async view(pos: number) {
      const { images, index } = await viewerImages(editor, pos);
      useImageUi.getState().openViewer(images, index);
    },

    setAlign(pos: number, align: ImageAlign) {
      update(pos, { align: align === "left" ? null : align });
    },

    editCaption(pos: number) {
      useImageUi.getState().editCaption(pos);
    },

    setAlt(pos: number, alt: string) {
      update(pos, { alt: alt.trim() || null });
    },

    remove(pos: number) {
      const node = imageAt(editor, pos);
      if (!node) return;
      editor
        .chain()
        .focus()
        .deleteRange({ from: pos, to: pos + node.nodeSize })
        .run();
    },

    async replace(pos: number) {
      try {
        const [saved] = await pickImages(false);
        if (!saved) return;
        const node = imageAt(editor, pos);
        if (!node) return;
        update(pos, { src: saved.path, alt: node.attrs.alt || saved.name, pending: null });
      } catch (error) {
        reportImageError(error);
      }
    },

    /** A web image the note links to: save a copy so it shows (and keep showing offline). */
    async saveWebCopy(pos: number) {
      const node = imageAt(editor, pos);
      const src = node?.attrs.src as string | undefined;
      if (!node || !src) return;
      // Edits may move the image while it downloads; find it again by its key.
      const key = newKey();
      update(pos, { pending: key });
      const settle = (attrs: Partial<ImageAttrs>) => {
        const hit = findPending(editor.state.doc, key);
        if (hit) update(hit.pos, { ...attrs, pending: null });
      };
      try {
        const saved = await saveImageSource(src);
        settle({ src: saved.path });
      } catch (error) {
        settle({});
        reportImageError(error);
      }
    },

    async copy(pos: number) {
      const url = await displayUrl(imageAt(editor, pos)?.attrs.src ?? null);
      if (!url) return;
      try {
        const blob = await asPng(await (await fetch(url)).blob());
        await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
        toast.success("Image copied");
      } catch {
        toast.error("Couldn't copy the image", {
          description: "Try again, or use Save image as and copy the file.",
        });
      }
    },

    async saveAs(pos: number) {
      const node = imageAt(editor, pos);
      const src = node?.attrs.src as string | undefined;
      if (!node || !src || !isAttachmentPath(src)) return;
      const name = `${(node.attrs.alt as string | null)?.replace(/[\\/:*?"<>|]+/g, "-").slice(0, 80) || "Image"}.${extensionOf(src)}`;
      try {
        if (isTauri) {
          const { save } = await import("@tauri-apps/plugin-dialog");
          const dest = await save({
            title: "Save image as",
            defaultPath: name,
            filters: [{ name: "Image", extensions: [extensionOf(src)] }],
          });
          if (!dest) return;
          await api.saveImageCopy(src, dest);
          toast.success(`Saved ${name}`, { description: dest });
        } else {
          const url = await displayUrl(src);
          if (!url) return;
          Object.assign(document.createElement("a"), { href: url, download: name }).click();
        }
      } catch (error) {
        toast.error("Couldn't save the image", { description: errorMessage(error) });
      }
    },

    async reveal(pos: number) {
      const src = imageAt(editor, pos)?.attrs.src as string | undefined;
      if (!src || !isAttachmentPath(src) || !isTauri) return;
      try {
        const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
        await revealItemInDir(await attachmentFile(src));
      } catch (error) {
        toast.error("Couldn't show the file", { description: errorMessage(error) });
      }
    },
  };
}

export type ImageActions = ReturnType<typeof imageActions>;
