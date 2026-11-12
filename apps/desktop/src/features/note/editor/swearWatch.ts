/**
 * Spots a swear the moment you finish typing it: the space, punctuation or Enter that comes
 * straight after typing its last letter. Retyping a space after an old word, or editing a word
 * that already went in the jar, never counts it again. The word is washed in amber while its coin
 * leaves; the wash fades and is removed after `WASH_MS`.
 *
 * Only typed text is seen: pastes, Markdown imports and Margin's own writing never reach
 * `handleTextInput`.
 */
import { Extension } from "@tiptap/core";
import { type EditorState, Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet, type EditorView } from "@tiptap/pm/view";
import { WASH_MS } from "@/features/swearjar/jar";

/** Return true when the word was a swear, so it gets the wash. `rect` is where it sits on screen. */
export type OnWordEnd = (word: string, rect: DOMRect) => boolean;

interface WatchState {
  /** The amber washes on screen. */
  wash: DecorationSet;
  /** Words already counted. They follow edits, and go when their text is deleted. */
  counted: DecorationSet;
  /** Where the last typed letter ended, as long as nothing else has changed the note since. */
  typedEnd: number | null;
}

type Meta = { add: { from: number; to: number; id: number } } | { remove: number };

export const swearKey = new PluginKey<WatchState>("swearWatch");

const WORD_END = /[\p{L}'’*]+$/u;
const BOUNDARY = /^[^\p{L}'’*]/u;

/** Each wash gets an id so its own timer removes it. */
let washes = 0;

/** The on-screen box around a document range. */
function rangeRect(view: EditorView, from: number, to: number): DOMRect {
  const a = view.coordsAtPos(from);
  const b = view.coordsAtPos(to);
  return new DOMRect(a.left, a.top, Math.max(1, b.right - a.left), Math.max(1, b.bottom - a.top));
}

export const SwearWatch = Extension.create<{ onWordEnd: OnWordEnd | null }>({
  name: "swearWatch",
  addOptions() {
    return { onWordEnd: null };
  },
  addProseMirrorPlugins() {
    const options = this.options;
    /** Set while typed text that ends in a letter is about to land: where that text will end. */
    let landing: number | null = null;

    // The word that ends at `at`, if you just finished typing it, it has not been counted, and it
    // is a swear: tell the jar, wash it once the key lands, and take the wash off once it fades
    const check = (view: EditorView, at: number) => {
      const state = swearKey.getState(view.state);
      if (!state || state.typedEnd !== at) return;
      const $at = view.state.doc.resolve(at);
      if (!$at.parent.isTextblock) return;
      const before = $at.parent.textBetween(0, $at.parentOffset, undefined, "￼");
      const word = WORD_END.exec(before)?.[0];
      if (!word) return;
      const from = at - word.length;
      if (state.counted.find(from, at).some((d) => d.from < at && d.to > from)) return;
      if (!options.onWordEnd?.(word, rangeRect(view, from, at))) return;
      const id = ++washes;
      const send = (meta: Meta) => {
        if (!view.isDestroyed) view.dispatch(view.state.tr.setMeta(swearKey, meta));
      };
      setTimeout(() => send({ add: { from, to: at, id } }));
      setTimeout(() => send({ remove: id }), WASH_MS);
    };

    return [
      new Plugin<WatchState>({
        key: swearKey,
        state: {
          init: () => ({ wash: DecorationSet.empty, counted: DecorationSet.empty, typedEnd: null }),
          apply(tr, prev): WatchState {
            // The typed letters remember where they ended; any other change to the note forgets it
            let typedEnd = prev.typedEnd;
            if (tr.docChanged) {
              typedEnd = landing;
              landing = null;
            }
            let wash = prev.wash.map(tr.mapping, tr.doc);
            let counted = prev.counted.map(tr.mapping, tr.doc);

            // A counted word: wash it, and remember it so it never counts twice
            const meta = tr.getMeta(swearKey) as Meta | undefined;
            if (meta && "add" in meta) {
              const { from, to, id } = meta.add;
              wash = wash.add(tr.doc, [Decoration.inline(from, to, { class: "nt-swear" }, { id })]);
              counted = counted.add(tr.doc, [Decoration.inline(from, to, {})]);
            } else if (meta && "remove" in meta) {
              wash = wash.remove(
                wash.find(undefined, undefined, (spec) => spec.id === meta.remove),
              );
            }
            return { wash, counted, typedEnd };
          },
        },
        props: {
          decorations: (state: EditorState) => swearKey.getState(state)?.wash,
          handleTextInput(view, from, _to, text) {
            if (BOUNDARY.test(text)) check(view, from);
            landing = WORD_END.test(text) ? from + text.length : null;
            return false;
          },
          handleKeyDown(view, event) {
            if (event.key === "Enter" && !event.isComposing) check(view, view.state.selection.from);
            return false;
          },
        },
      }),
    ];
  },
});
