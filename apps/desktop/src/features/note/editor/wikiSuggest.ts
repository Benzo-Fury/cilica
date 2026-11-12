/**
 * Typing `[[` in a note suggests notes to link. This plugin only tracks the query (the text after
 * `[[` up to the caret) and hands keys to whoever renders the list (`LinkSuggest`); picking an
 * entry calls `insertWikiLink`.
 */
import { Extension } from "@tiptap/core";
import { type EditorState, Plugin, PluginKey } from "@tiptap/pm/state";
import type { EditorView } from "@tiptap/pm/view";
import type { WikiLinkAttrs } from "./wikiLink";

export interface SuggestState {
  /** Where `[[` (or `![[`) starts and where the caret is. */
  from: number;
  to: number;
  query: string;
  embed: boolean;
}

export interface SuggestHandlers {
  onChange: (state: SuggestState | null, view: EditorView) => void;
  /** Arrow keys, Enter, Tab and Escape while the list shows. True when handled. */
  onKey: (key: string) => boolean;
}

interface PluginState {
  open: SuggestState | null;
  /** Where the `[[` dismissed with Esc starts: it stays closed until that changes. */
  dismissedAt: number | null;
}

export const suggestKey = new PluginKey<PluginState>("wikiSuggest");

const QUERY = /(!?)\[\[([^[\]|#\n]{0,80})$/;

function find(state: EditorState): SuggestState | null {
  const { selection } = state;
  if (!selection.empty) return null;
  const $from = selection.$from;
  if (!$from.parent.isTextblock || $from.parent.type.spec.code) return null;
  const before = $from.parent.textBetween(0, $from.parentOffset, "\n", "￼");
  const m = QUERY.exec(before);
  if (!m) return null;
  return {
    from: selection.from - m[0].length,
    to: selection.from,
    query: m[2] ?? "",
    embed: m[1] === "!",
  };
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    wikiSuggest: {
      /** Replace the typed `[[query` with a link. */
      insertWikiLink: (range: { from: number; to: number }, attrs: WikiLinkAttrs) => ReturnType;
      /** Hide the list until the typed `[[` changes. */
      dismissWikiSuggest: () => ReturnType;
    };
  }
}

export const WikiSuggest = Extension.create<{
  /** A getter, not an object: TipTap deep-copies option objects when it configures them. */
  handlers: () => SuggestHandlers | null;
}>({
  name: "wikiSuggest",

  addOptions() {
    return { handlers: () => null };
  },

  addCommands() {
    return {
      insertWikiLink:
        (range, attrs) =>
        ({ chain }) =>
          chain()
            .insertContentAt(range, [
              { type: "wikiLink", attrs },
              { type: "text", text: " " },
            ])
            .run(),
      dismissWikiSuggest:
        () =>
        ({ tr, dispatch }) => {
          if (dispatch) dispatch(tr.setMeta(suggestKey, "dismiss"));
          return true;
        },
    };
  },

  addProseMirrorPlugins() {
    const handlers = this.options.handlers;
    return [
      new Plugin<PluginState>({
        key: suggestKey,
        state: {
          init: (): PluginState => ({ open: null, dismissedAt: null }),
          apply: (tr, prev, _old, next): PluginState => {
            const found = find(next);
            if (tr.getMeta(suggestKey) === "dismiss") {
              return { open: null, dismissedAt: found?.from ?? null };
            }
            if (found && found.from === prev.dismissedAt)
              return { open: null, dismissedAt: found.from };
            return { open: found, dismissedAt: null };
          },
        },
        view: () => {
          let last: SuggestState | null = null;
          return {
            update: (view) => {
              const open = suggestKey.getState(view.state)?.open ?? null;
              const same =
                open === last ||
                (open &&
                  last &&
                  open.from === last.from &&
                  open.to === last.to &&
                  open.query === last.query);
              if (same) return;
              last = open;
              handlers()?.onChange(open, view);
            },
            destroy: () => {
              last = null;
            },
          };
        },
        props: {
          handleKeyDown: (view, event) => {
            if (!suggestKey.getState(view.state)?.open) return false;
            if (!["ArrowDown", "ArrowUp", "Enter", "Tab", "Escape"].includes(event.key))
              return false;
            if (event.metaKey || event.ctrlKey || event.altKey) return false;
            return handlers()?.onKey(event.key) ?? false;
          },
        },
      }),
    ];
  },
});
