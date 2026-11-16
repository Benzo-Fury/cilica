/**
 * Offers to link a note title the moment you finish typing it: the space, punctuation or Enter
 * straight after its last letter. Tab (or a click on the hint) turns the words into a `[[link]]`;
 * anything else you type, moving the caret, Esc or leaving the editor puts it away. It never
 * takes focus or a key it does not use, and a title it offered is not offered again in this note
 * until it is reopened (the header's suggestions still have it).
 *
 * Only typed text is seen: pastes, Markdown imports and Cilica's own writing never reach
 * `handleTextInput`. Finding the title is left to `match` (titles live in the link resolver).
 */
import { Extension } from "@tiptap/core";
import type { Node as PMNode } from "@tiptap/pm/model";
import { type EditorState, Plugin, PluginKey, type Transaction } from "@tiptap/pm/state";
import type { EditorView } from "@tiptap/pm/view";
import { linkKey, wikiPage } from "@/lib/links";
import { suggestKey } from "./wikiSuggest";

export interface InlineLinkMatch {
  noteId: string;
  title: string;
  /** `linkKey` of the title. */
  key: string;
  /** The words to link, as typed; the link shows them, so the sentence reads the same. */
  typed: string;
  length: number;
  /** Typed characters after the words that stay outside the link: the `'s` of "Lisbon's". */
  trail: number;
  /** Typed as the title is written (bar case), so `[[typed]]` finds it; else `[[Title|typed]]`. */
  exact: boolean;
}

/** Find a title that `before` ends with; `isLinked` says whether this note already links it. */
export type InlineLinkMatcher = (
  before: string,
  isLinked: (key: string) => boolean,
) => InlineLinkMatch | null;

export interface InlineLinkHint {
  /** The typed words: what Tab replaces. */
  from: number;
  to: number;
  noteId: string;
  title: string;
  /** `linkKey` of the title. */
  key: string;
  typed: string;
  /** What the link points at and what it shows (null: the target as written). */
  target: string;
  alias: string | null;
}

/** How a hint ended: linked, gone past (typed on, moved away) or put away with Esc. */
export type InlineLinkOutcome = "taken" | "passed" | "refused";

export interface InlineLinkHandlers {
  onChange: (hint: InlineLinkHint | null, view: EditorView) => void;
  /**
   * Called once per hint that you answered. Going past it is reported at the next word end, and
   * not at all when that word grows the same title ("cell" → "cell membrane"). A hint put away by
   * leaving the editor or scrolling is not reported.
   */
  onOutcome?: (hint: InlineLinkHint, outcome: InlineLinkOutcome) => void;
}

type Ending = InlineLinkOutcome | "away";

interface HintState {
  hint: InlineLinkHint | null;
  /** The hint this transaction put away, and why. */
  ended: { hint: InlineLinkHint; why: Ending } | null;
  /** Where the caret was when the hint showed: moving it puts the hint away. */
  caret: number | null;
  /** Where the last typed letter ended, as long as nothing else has changed the note since. */
  typedEnd: number | null;
}

/** `close`: put away without an answer (blur, scroll); the others say how you answered. */
type Meta = "close" | "taken" | "refused";

export const inlineLinkKey = new PluginKey<HintState>("inlineLinkHint");

const WORD_END = /[\p{L}\p{N}'’]$/u;
// Not ’: it is as often an apostrophe ("Lisbon’s") as a closing quote.
const BOUNDARY = /^[\s.,;:!?)\]}"”…]/u;

/** Link keys this document already links to. */
function linkedKeys(doc: PMNode): Set<string> {
  const keys = new Set<string>();
  doc.descendants((node) => {
    if (node.type.name === "wikiLink") {
      const page = wikiPage(String(node.attrs.target ?? ""));
      if (page) keys.add(linkKey(page));
    }
    return node.isBlock;
  });
  return keys;
}

/** Turn the hinted words into a link on `tr`. False when there is no hint or the words changed. */
function linkHinted(state: EditorState, tr: Transaction): boolean {
  const hint = inlineLinkKey.getState(state)?.hint;
  const type = state.schema.nodes.wikiLink;
  if (!hint || !type || state.doc.textBetween(hint.from, hint.to) !== hint.typed) return false;
  // The link keeps the words as typed: `[[cytoplasm]]` finds "Cytoplasm" like any link does.
  tr.replaceWith(
    hint.from,
    hint.to,
    type.create({ target: hint.target, alias: hint.alias, embed: false }),
  );
  tr.setMeta(inlineLinkKey, "taken");
  return true;
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    inlineLinkHint: {
      /** Turn the hinted words into a `[[link]]`. False when there is no hint. */
      acceptInlineLink: () => ReturnType;
      /** Put the hint away. */
      dismissInlineLink: () => ReturnType;
    };
  }
}

export const InlineLinkHintExtension = Extension.create<{
  match: InlineLinkMatcher | null;
  /** A getter, not an object: TipTap deep-copies option objects when it configures them. */
  handlers: () => InlineLinkHandlers | null;
}>({
  name: "inlineLinkHint",
  // Ahead of list indenting, so Tab links while a hint shows (and only then).
  priority: 1000,

  addOptions() {
    return { match: null, handlers: () => null };
  },

  addCommands() {
    return {
      acceptInlineLink:
        () =>
        ({ state, tr }) =>
          linkHinted(state, tr),
      dismissInlineLink:
        () =>
        ({ tr, dispatch }) => {
          if (dispatch) dispatch(tr.setMeta(inlineLinkKey, "close"));
          return true;
        },
    };
  },

  addProseMirrorPlugins() {
    const { match, handlers } = this.options;
    if (!match) return [];
    /** Set while typed text that ends in a letter is about to land: where that text will end. */
    let landing: number | null = null;
    /** A hint found on the key that ends a word, shown once that key lands. */
    let pending: InlineLinkHint | null = null;
    /** Titles offered in this editor already: each is offered once. */
    const offered = new Set<string>();
    /** A hint you typed past, reported as passed at the next word end unless that word grows it. */
    let unsettled: InlineLinkHint | null = null;
    /** The text landing now ends a word past the hint, so going past it is already settled. */
    let pastNow = false;
    const settle = () => {
      const hint = unsettled;
      unsettled = null;
      if (hint) handlers()?.onOutcome?.(hint, "passed");
    };

    // The title that ends where a word was just finished: at `at` (a single key), or at the end
    // of `inserted`, text landing at `at` in one go (autocorrect, text replacements, dictation).
    // Positions are worked out for the document after the key lands, which keeps everything
    // before `at` where it is.
    const check = (view: EditorView, at: number, inserted = "") => {
      pending = find(view, at, inserted);
      if (unsettled && pending?.from !== unsettled.from) settle();
      if (!pending) return;
      const hint = pending;
      // The key lands synchronously; if it changed nothing, the hint is not for a later edit.
      queueMicrotask(() => {
        if (pending === hint) pending = null;
      });
    };
    const find = (view: EditorView, at: number, inserted: string): InlineLinkHint | null => {
      const state = inlineLinkKey.getState(view.state);
      if (!state || (!inserted && state.typedEnd !== at)) return null;
      if (suggestKey.getState(view.state)?.open) return null; // inside `[[`: that list has it
      const $at = view.state.doc.resolve(at);
      if (!$at.parent.isTextblock || $at.parent.type.spec.code) return null;
      if ($at.marks().some((m) => m.type.name === "code" || m.type.name === "link")) return null;
      const before = $at.parent.textBetween(0, $at.parentOffset, undefined, "\uFFFC") + inserted;
      let linked: Set<string> | null = null;
      const found = match(before, (key) => {
        if (offered.has(key)) return true;
        linked ??= linkedKeys(view.state.doc);
        return linked.has(key);
      });
      if (!found) return null;
      offered.add(found.key);
      const to = at + inserted.length - found.trail;
      return {
        from: to - found.length,
        to,
        noteId: found.noteId,
        title: found.title,
        key: found.key,
        typed: found.typed,
        // `[[cytoplasm]]` finds "Cytoplasm"; "cytoplasms" needs `[[Cytoplasm|cytoplasms]]`.
        target: found.exact ? found.typed : found.title,
        alias: found.exact ? null : found.typed,
      };
    };

    return [
      new Plugin<HintState>({
        key: inlineLinkKey,
        state: {
          init: () => ({ hint: null, ended: null, caret: null, typedEnd: null }),
          apply(tr, prev, _old, next: EditorState): HintState {
            let typedEnd = prev.typedEnd;
            let hint = prev.hint;
            let caret = prev.caret;
            if (tr.docChanged) {
              typedEnd = landing;
              landing = null;
              // The key that ended the word landed: show its hint. Any other edit puts it away.
              const landed = pending;
              pending = null;
              if (
                landed &&
                landed.to <= next.doc.content.size &&
                next.doc.textBetween(landed.from, landed.to) === landed.typed
              ) {
                hint = landed;
                caret = next.selection.from;
              } else {
                hint = null;
              }
            } else if (hint && (!next.selection.empty || next.selection.from !== caret)) {
              hint = null;
            }
            const meta = tr.getMeta(inlineLinkKey) as Meta | undefined;
            if (meta) hint = null;
            const ended: HintState["ended"] =
              prev.hint && hint !== prev.hint
                ? { hint: prev.hint, why: meta === "close" ? "away" : (meta ?? "passed") }
                : null;
            return { hint, ended, caret: hint ? caret : null, typedEnd };
          },
        },
        view: () => {
          let last: InlineLinkHint | null = null;
          return {
            update: (view) => {
              const state = inlineLinkKey.getState(view.state);
              const hint = state?.hint ?? null;
              if (hint === last) return;
              const ended = state?.ended;
              if (ended && ended.hint === last) {
                if (ended.why === "passed") {
                  settle();
                  unsettled = ended.hint;
                  if (pastNow) settle();
                } else if (ended.why !== "away") {
                  handlers()?.onOutcome?.(ended.hint, ended.why);
                }
              }
              // The word you typed on with grew into a longer title: not an ignore.
              if (hint && unsettled?.from === hint.from) unsettled = null;
              last = hint;
              handlers()?.onChange(hint, view);
            },
            destroy: () => {
              settle();
              last = null;
            },
          };
        },
        props: {
          handleTextInput(view, from, to, text) {
            pastNow = false;
            if (text.length === 1) {
              if (from === to && BOUNDARY.test(text)) check(view, from);
              else pending = null;
            } else if (BOUNDARY.test(text.slice(-1)) && WORD_END.test(text.slice(0, -1))) {
              check(view, from, text.slice(0, -1));
            } else {
              pending = null;
              // A word ends inside text that lands in one go: a hint typed past is settled.
              if (/[\s.,;:!?…]/u.test(text)) {
                settle();
                pastNow = true;
              }
            }
            landing = WORD_END.test(text) ? from + text.length : null;
            return false;
          },
          handleKeyDown(view, event) {
            const hint = inlineLinkKey.getState(view.state)?.hint;
            const plain = !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey;
            if (hint && plain && !event.isComposing) {
              if (event.key === "Tab") {
                const tr = view.state.tr;
                if (!linkHinted(view.state, tr)) return false;
                view.dispatch(tr);
                return true;
              }
              if (event.key === "Escape") {
                view.dispatch(view.state.tr.setMeta(inlineLinkKey, "refused"));
                return true;
              }
            }
            if (event.key === "Enter" && !event.isComposing && plain) {
              check(view, view.state.selection.from);
            }
            return false;
          },
          handleDOMEvents: {
            blur(view) {
              if (inlineLinkKey.getState(view.state)?.hint) {
                view.dispatch(view.state.tr.setMeta(inlineLinkKey, "close"));
              }
              return false;
            },
          },
        },
      }),
    ];
  },
});
