/**
 * Visual treatment for text Cilica streams into a note: the newest words fade in
 * (`nt-stream-new`), and when the stream finishes the whole written range keeps an
 * `accent-soft` wash that fades out over 2s (`nt-ai-settle`).
 */
import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

type Meta =
  | { from: number; to: number; className: string }
  | { ranges: { from: number; to: number }[]; className: string }
  | { clear: true };

export const streamKey = new PluginKey<DecorationSet>("streamHighlight");

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    streamHighlight: {
      markStreamed: (from: number, to: number, className: string) => ReturnType;
      /** Wash several ranges at once (an accepted AI edit's changed sentences). */
      markRanges: (ranges: { from: number; to: number }[], className: string) => ReturnType;
      clearStreamed: () => ReturnType;
    };
  }
}

export const StreamHighlight = Extension.create({
  name: "streamHighlight",
  addCommands() {
    return {
      markStreamed:
        (from, to, className) =>
        ({ tr, dispatch }) => {
          if (dispatch) dispatch(tr.setMeta(streamKey, { from, to, className } satisfies Meta));
          return true;
        },
      markRanges:
        (ranges, className) =>
        ({ tr, dispatch }) => {
          if (dispatch) dispatch(tr.setMeta(streamKey, { ranges, className } satisfies Meta));
          return true;
        },
      clearStreamed:
        () =>
        ({ tr, dispatch }) => {
          if (dispatch) dispatch(tr.setMeta(streamKey, { clear: true } satisfies Meta));
          return true;
        },
    };
  },
  addProseMirrorPlugins() {
    return [
      new Plugin<DecorationSet>({
        key: streamKey,
        state: {
          init: () => DecorationSet.empty,
          apply: (tr, set) => {
            const meta = tr.getMeta(streamKey) as Meta | undefined;
            if (meta && "clear" in meta) return DecorationSet.empty;
            if (meta && "ranges" in meta) {
              const size = tr.doc.content.size;
              const decorations = meta.ranges
                .map(({ from, to }) => ({ from: Math.max(0, from), to: Math.min(to, size) }))
                .filter(({ from, to }) => from < to)
                .map(({ from, to }) => Decoration.inline(from, to, { class: meta.className }));
              return DecorationSet.create(tr.doc, decorations);
            }
            if (meta) {
              const to = Math.min(meta.to, tr.doc.content.size);
              const from = Math.max(0, Math.min(meta.from, to));
              if (from >= to) return DecorationSet.empty;
              return DecorationSet.create(tr.doc, [
                Decoration.inline(from, to, { class: meta.className }),
              ]);
            }
            return tr.docChanged ? set.map(tr.mapping, tr.doc) : set;
          },
        },
        props: { decorations: (state) => streamKey.getState(state) },
      }),
    ];
  },
});
