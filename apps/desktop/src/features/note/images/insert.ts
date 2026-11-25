/** Insert image (⌘⇧I and the note menu): pick files, then add each image after the current block. */
import type { Editor } from "@tiptap/core";
import { pickImages, reportImageError } from "./saver";

export async function insertImagesFromPicker(editor: Editor) {
  try {
    const saved = await pickImages(true);
    if (saved.length === 0 || editor.isDestroyed) return;
    const type = editor.state.schema.nodes.image;
    if (!type) return;
    const { state } = editor;
    // After the caret's block, so a paragraph is never split in two.
    const $at = state.doc.resolve(state.selection.to);
    let at = $at.depth > 0 ? $at.after(1) : $at.pos;
    const tr = state.tr;
    for (const image of saved) {
      const node = type.create({ src: image.path, alt: image.name });
      tr.insert(at, node);
      at += node.nodeSize;
    }
    editor.view.dispatch(tr.scrollIntoView());
    editor.view.focus();
  } catch (error) {
    reportImageError(error);
  }
}
