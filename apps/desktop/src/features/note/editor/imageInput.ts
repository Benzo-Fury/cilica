/**
 * How images get into a note:
 * - pasted image data (a screenshot, an image copied from another app) and image files dropped
 *   in a browser are shown at once from memory and saved as attachments in the background;
 * - images that arrive as web addresses or `data:` URIs (pasted HTML or Markdown, `insertContent`)
 *   are downloaded or saved once, so the note never loads anything from the web.
 *
 * Saving goes through the `saver` the editor is configured with, so this file stays free of IPC.
 * In Tauri, files dropped from Finder or Explorer arrive as paths instead (see useImageFileDrop).
 */
import { Extension } from "@tiptap/core";
import type { Node as PMNode } from "@tiptap/pm/model";
import { type EditorState, Plugin, PluginKey, type Transaction } from "@tiptap/pm/state";
import type { EditorView } from "@tiptap/pm/view";
import { needsSaving } from "./image";

export interface SavedImageRef {
  /** `attachments/<hash>.<ext>` */
  path: string;
  /** Suggested alt text (the file's name). */
  name: string;
}

export interface ImageSaver {
  /** Save pasted or dropped image data. */
  file(file: File): Promise<SavedImageRef>;
  /** Download a web image, or save a `data:` URI. */
  source(src: string): Promise<SavedImageRef>;
  /** Saving failed: tell the person what went wrong. */
  failed(error: unknown): void;
}

export const IMAGE_TYPES = /^image\/(png|jpe?g|gif|webp|avif|bmp|heic|heif|tiff)$/i;
export const IMAGE_EXTENSIONS = [
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "avif",
  "bmp",
  "heic",
  "heif",
  "tif",
  "tiff",
];

/** Image files in a clipboard or drop, in order. */
export function imageFiles(list: FileList | null | undefined): File[] {
  return Array.from(list ?? []).filter(
    (f) => IMAGE_TYPES.test(f.type) || hasImageExtension(f.name),
  );
}

export function hasImageExtension(name: string): boolean {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  return name.includes(".") && IMAGE_EXTENSIONS.includes(ext);
}

/** Pasted HTML that is only an image (or images): take the image data, not the address. */
function htmlIsOnlyImages(html: string): boolean {
  if (!html) return true;
  const doc = new DOMParser().parseFromString(html, "text/html");
  return (doc.body.textContent ?? "").trim() === "";
}

let nextKey = 0;
export const newKey = () => `img-${Date.now().toString(36)}-${(nextKey++).toString(36)}`;

/** Find the image that is waiting on `key`, wherever edits have moved it. */
export function findPending(doc: PMNode, key: string): { pos: number; node: PMNode } | null {
  let found: { pos: number; node: PMNode } | null = null;
  doc.descendants((node, pos) => {
    if (found) return false;
    if (node.type.name === "image" && node.attrs.pending === key) found = { pos, node };
    return !found;
  });
  return found;
}

/** Point the waiting image at its saved file, or drop it when saving failed and it had no source. */
function settle(view: EditorView, key: string, result: SavedImageRef | null, keepOnFail: boolean) {
  if (view.isDestroyed) return;
  const hit = findPending(view.state.doc, key);
  if (!hit) return;
  const { tr } = view.state;
  if (result) {
    const alt = hit.node.attrs.alt || result.name || null;
    tr.setNodeMarkup(hit.pos, undefined, {
      ...hit.node.attrs,
      src: result.path,
      alt,
      pending: null,
    });
    const old = hit.node.attrs.src as string | null;
    if (old?.startsWith("blob:")) URL.revokeObjectURL(old);
  } else if (keepOnFail) {
    tr.setNodeMarkup(hit.pos, undefined, { ...hit.node.attrs, pending: null });
  } else {
    tr.delete(hit.pos, hit.pos + hit.node.nodeSize);
  }
  // Part of the edit that brought the image in, as far as undo is concerned.
  tr.setMeta("addToHistory", false);
  view.dispatch(tr);
}

/** Insert `files` at `pos` (or the selection), shown from memory until each is saved. */
export function insertImageFiles(view: EditorView, saver: ImageSaver, files: File[], pos?: number) {
  const type = view.state.schema.nodes.image;
  if (!type || files.length === 0) return;
  const jobs = files.map((file) => ({ file, key: newKey(), url: URL.createObjectURL(file) }));
  const nodes = jobs.map((job) =>
    type.create({
      src: job.url,
      alt: job.file.name.replace(/\.[^.]+$/, "") || null,
      pending: job.key,
    }),
  );
  const { tr } = view.state;
  if (pos === undefined) tr.replaceSelectionWith(nodes[0] as PMNode, false);
  else tr.insert(pos, nodes[0] as PMNode);
  let at = tr.mapping.map(pos ?? view.state.selection.to);
  for (const node of nodes.slice(1)) {
    tr.insert(at, node);
    at += node.nodeSize;
  }
  view.dispatch(tr.scrollIntoView());
  for (const job of jobs) {
    saver.file(job.file).then(
      (saved) => settle(view, job.key, saved, false),
      (error) => {
        saver.failed(error);
        URL.revokeObjectURL(job.url);
        settle(view, job.key, null, false);
      },
    );
  }
}

/**
 * Images in `doc` whose source still has to be saved, counted by source. Ones already being
 * saved count too, so a failed download that clears its key is not taken for a new image.
 */
function unsavedSources(doc: PMNode): Map<string, number> {
  const out = new Map<string, number>();
  doc.descendants((node) => {
    if (node.type.name === "image" && needsSaving(node.attrs.src)) {
      out.set(node.attrs.src, (out.get(node.attrs.src) ?? 0) + 1);
    }
  });
  return out;
}

const key = new PluginKey("imageInput");

export const ImageInput = Extension.create<{ saver: () => ImageSaver | null }>({
  name: "imageInput",

  addOptions() {
    return { saver: () => null };
  },

  addProseMirrorPlugins() {
    const getSaver = this.options.saver;
    let viewRef: EditorView | null = null;
    return [
      new Plugin({
        key,
        view(view) {
          viewRef = view;
          return { destroy: () => (viewRef = null) };
        },
        props: {
          handlePaste(view, event) {
            const saver = getSaver();
            const data = event.clipboardData;
            if (!saver || !data || !view.editable) return false;
            const files = imageFiles(data.files);
            if (files.length === 0 || !htmlIsOnlyImages(data.getData("text/html"))) return false;
            if (view.state.selection.$from.parent.type.spec.code) return false;
            insertImageFiles(view, saver, files);
            return true;
          },
          handleDrop(view, event, _slice, moved) {
            const saver = getSaver();
            if (!saver || moved || !view.editable) return false;
            const files = imageFiles(event.dataTransfer?.files);
            if (files.length === 0) return false;
            const at = view.posAtCoords({ left: event.clientX, top: event.clientY });
            event.preventDefault();
            insertImageFiles(
              view,
              saver,
              files,
              at ? blockBoundary(view.state, at.pos) : undefined,
            );
            return true;
          },
        },
        // A person's edit brought in images from the web or as data: save each once.
        appendTransaction(
          transactions: readonly Transaction[],
          oldState: EditorState,
          newState: EditorState,
        ) {
          const saver = getSaver();
          if (!saver) return null;
          const edited = transactions.some((t) => t.docChanged && !t.getMeta("preventUpdate"));
          if (!edited) return null;
          const before = unsavedSources(oldState.doc);
          const tr = newState.tr;
          const started: { key: string; src: string }[] = [];
          newState.doc.descendants((node, pos) => {
            if (node.type.name !== "image" || !needsSaving(node.attrs.src)) return;
            const src = node.attrs.src as string;
            const left = before.get(src) ?? 0;
            if (left > 0) {
              // Already in the note before this edit (an old web image): leave it alone.
              before.set(src, left - 1);
              return;
            }
            const job = { key: newKey(), src };
            started.push(job);
            tr.setNodeMarkup(pos, undefined, { ...node.attrs, pending: job.key });
          });
          if (started.length === 0) return null;
          queueMicrotask(() => {
            for (const job of started) {
              saver.source(job.src).then(
                (saved) => viewRef && settle(viewRef, job.key, saved, true),
                (error) => {
                  saver.failed(error);
                  if (viewRef) settle(viewRef, job.key, null, true);
                },
              );
            }
          });
          return tr.setMeta("addToHistory", false);
        },
      }),
    ];
  },
});

/** The nearest position between top-level blocks to `pos`, so a dropped image never splits a paragraph. */
export function blockBoundary(state: EditorState, pos: number): number {
  const $pos = state.doc.resolve(pos);
  if ($pos.depth === 0) return pos;
  const start = $pos.before(1);
  const end = $pos.after(1);
  return pos - start <= end - pos ? start : end;
}
