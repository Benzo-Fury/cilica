/**
 * `#tags` in a note, as Obsidian reads them (letters, numbers, `_`, `-`, `/`, not all numbers,
 * after a space or at the start of a line). They stay plain text in the Markdown; this only styles
 * them and opens the graph on that tag when clicked.
 */
import { Extension } from "@tiptap/core";
import type { Node as PMNode } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

const TAG = /(^|\s)#([\p{L}\p{N}_\-/]*[\p{L}_\-/][\p{L}\p{N}_\-/]*)/gu;

const tagsKey = new PluginKey<DecorationSet>("tags");

function decorate(doc: PMNode): DecorationSet {
  const decorations: Decoration[] = [];
  doc.descendants((node, pos, parent) => {
    if (parent?.type.spec.code) return false;
    if (!node.isText || !node.text) return;
    if (node.marks.some((m) => m.type.name === "code" || m.type.name === "link")) return;
    for (const m of node.text.matchAll(TAG)) {
      const tag = (m[2] ?? "").replace(/\/+$/, "");
      if (!tag || tag.startsWith("/")) continue;
      const from = pos + (m.index ?? 0) + (m[1]?.length ?? 0);
      decorations.push(
        Decoration.inline(from, from + 1 + tag.length, { class: "nt-tag", "data-tag": tag }),
      );
    }
  });
  return DecorationSet.create(doc, decorations);
}

export const Tags = Extension.create<{ onOpen: ((tag: string) => void) | null }>({
  name: "tags",
  addOptions() {
    return { onOpen: null };
  },
  addProseMirrorPlugins() {
    const options = this.options;
    return [
      new Plugin<DecorationSet>({
        key: tagsKey,
        state: {
          init: (_, state) => decorate(state.doc),
          apply: (tr, set) => (tr.docChanged ? decorate(tr.doc) : set),
        },
        props: {
          decorations: (state) => tagsKey.getState(state),
          handleClick: (_view, _pos, event) => {
            const el = (event.target as HTMLElement | null)?.closest<HTMLElement>(".nt-tag");
            const tag = el?.dataset.tag;
            if (!tag || !options.onOpen || !(event.metaKey || event.ctrlKey)) return false;
            options.onOpen(tag);
            return true;
          },
        },
      }),
    ];
  },
});
