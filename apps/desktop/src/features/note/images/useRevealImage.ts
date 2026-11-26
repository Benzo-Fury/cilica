/**
 * Opening a note from a ⌘K image match: once the note's content is in the editor, select that
 * image and scroll it to the middle of the page, then forget the request.
 */
import type { Editor } from "@tiptap/core";
import { NodeSelection } from "@tiptap/pm/state";
import { useEffect } from "react";
import { useImageUi } from "./store";

/** The position of the first image with this `src`, or null. */
export function findImage(editor: Editor, src: string): number | null {
  let found: number | null = null;
  editor.state.doc.descendants((node, pos) => {
    if (found !== null) return false;
    if (node.type.name === "image" && node.attrs.src === src) found = pos;
    return true;
  });
  return found;
}

export function useRevealImage(editor: Editor | null) {
  const reveal = useImageUi((s) => s.reveal);
  useEffect(() => {
    if (!editor || !reveal) return;
    const attempt = () => {
      if (editor.isDestroyed) return true;
      const pos = findImage(editor, reveal);
      if (pos === null) return false;
      editor.view.dispatch(
        editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, pos)),
      );
      const dom = editor.view.nodeDOM(pos);
      if (dom instanceof HTMLElement) dom.scrollIntoView({ block: "center" });
      editor.view.focus();
      useImageUi.getState().revealImage(null);
      return true;
    };
    if (attempt()) return;
    // The note's body arrives a moment after the editor mounts.
    const onUpdate = () => {
      if (attempt()) editor.off("transaction", onUpdate);
    };
    editor.on("transaction", onUpdate);
    // Give up after a while so a stale request can't grab a later note.
    const t = setTimeout(() => {
      editor.off("transaction", onUpdate);
      useImageUi.getState().revealImage(null);
    }, 3000);
    return () => {
      clearTimeout(t);
      editor.off("transaction", onUpdate);
    };
  }, [editor, reveal]);
}
