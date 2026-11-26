/**
 * Image files dragged in from Finder or File Explorer. Inside Tauri the window takes file drops
 * itself and hands over paths (the webview never sees them), so this shows the accent line
 * between blocks while files hover over the note, and on drop inserts each image there: a
 * placeholder holds its place while the backend reads and saves the file.
 * In the browser preview the editor's own drop handler covers this (see imageInput).
 */
import type { Editor } from "@tiptap/core";
import type { Node as PMNode } from "@tiptap/pm/model";
import { useEffect } from "react";
import { api, isTauri } from "@/ipc/client";
import { blockBoundary, findPending, hasImageExtension, newKey } from "../editor/imageInput";
import { reportImageError } from "./saver";

/** The gap between blocks nearest to a viewport point, and where its line goes. */
function dropPoint(editor: Editor, x: number, y: number) {
  const view = editor.view;
  const box = view.dom.getBoundingClientRect();
  // Only over the note column (with some slack either side).
  if (x < box.left - 64 || x > box.right + 64) return null;
  const hit = view.posAtCoords({
    left: Math.min(Math.max(x, box.left + 1), box.right - 1),
    top: y,
  });
  const pos = hit ? blockBoundary(view.state, hit.pos) : view.state.doc.content.size;
  const nodeBefore = view.state.doc.resolve(pos).nodeBefore;
  const before = nodeBefore ? view.nodeDOM(pos - nodeBefore.nodeSize) : null;
  const after = view.nodeDOM(pos);
  const top =
    after instanceof HTMLElement
      ? after.getBoundingClientRect().top - 4
      : before instanceof HTMLElement
        ? before.getBoundingClientRect().bottom + 4
        : box.top;
  return { pos, line: { left: box.left, width: box.width, top } };
}

/** Insert placeholders at `pos`, then save each file and point its image at the result. */
export function insertImagePaths(editor: Editor, paths: string[], pos: number) {
  const type = editor.state.schema.nodes.image;
  if (!type) return;
  const jobs = paths.map((path) => ({ path, key: newKey() }));
  const { tr } = editor.state;
  let at = pos;
  for (const job of jobs) {
    const node: PMNode = type.create({ src: null, pending: job.key });
    tr.insert(at, node);
    at += node.nodeSize;
  }
  editor.view.dispatch(tr.scrollIntoView());
  for (const job of jobs) {
    api.importImageFile(job.path).then(
      (saved) => {
        const hit = findPending(editor.state.doc, job.key);
        if (!hit || editor.isDestroyed) return;
        editor.view.dispatch(
          editor.state.tr
            .setNodeMarkup(hit.pos, undefined, {
              ...hit.node.attrs,
              src: saved.path,
              alt: saved.name,
              pending: null,
            })
            .setMeta("addToHistory", false),
        );
      },
      (error) => {
        reportImageError(error);
        const hit = findPending(editor.state.doc, job.key);
        if (!hit || editor.isDestroyed) return;
        editor.view.dispatch(
          editor.state.tr
            .delete(hit.pos, hit.pos + hit.node.nodeSize)
            .setMeta("addToHistory", false),
        );
      },
    );
  }
}

export function useImageFileDrop(editor: Editor | null) {
  useEffect(() => {
    if (!isTauri || !editor) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    const line = document.createElement("div");
    line.className = "nt-image-dropline";
    let images = false;

    const show = (x: number, y: number) => {
      const point = images && editor.isEditable ? dropPoint(editor, x, y) : null;
      if (!point) {
        line.remove();
        return;
      }
      Object.assign(line.style, {
        left: `${point.line.left}px`,
        width: `${point.line.width}px`,
        top: `${Math.round(point.line.top)}px`,
      });
      if (!line.isConnected) document.body.append(line);
    };

    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview()
        .onDragDropEvent((event) => {
          const p = event.payload;
          // Positions arrive in physical pixels.
          const scale = window.devicePixelRatio || 1;
          if (p.type === "enter") {
            images = p.paths.some(hasImageExtension);
            show(p.position.x / scale, p.position.y / scale);
          } else if (p.type === "over") {
            show(p.position.x / scale, p.position.y / scale);
          } else if (p.type === "leave") {
            line.remove();
          } else if (p.type === "drop") {
            line.remove();
            const paths = p.paths.filter(hasImageExtension);
            if (paths.length === 0 || !editor.isEditable || editor.isDestroyed) return;
            const point = dropPoint(editor, p.position.x / scale, p.position.y / scale);
            if (point) insertImagePaths(editor, paths, point.pos);
          }
        })
        .then((off) => {
          if (disposed) off();
          else unlisten = off;
        }),
    );
    return () => {
      disposed = true;
      line.remove();
      unlisten?.();
    };
  }, [editor]);
}
