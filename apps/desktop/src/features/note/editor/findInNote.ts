/**
 * Find in note (⌘F): highlights every match with decorations and tracks the current one.
 * Commands: setFindQuery(query), findNext(), findPrevious(), clearFind().
 */
import { Extension } from "@tiptap/core";
import type { Node as PMNode } from "@tiptap/pm/model";
import { Plugin, PluginKey, TextSelection } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

export interface FindState {
  query: string;
  matches: { from: number; to: number }[];
  current: number;
  decorations: DecorationSet;
}

export const findKey = new PluginKey<FindState>("findInNote");

type Meta = { query?: string; current?: number };

export function findMatches(doc: PMNode, query: string): { from: number; to: number }[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const matches: { from: number; to: number }[] = [];
  doc.descendants((node, pos) => {
    if (!node.isTextblock) return true;
    // Concatenate the block's text with a position map so matches can span marks.
    let text = "";
    const offsets: number[] = [];
    node.forEach((child, offset) => {
      if (child.isText && child.text) {
        for (let i = 0; i < child.text.length; i++) offsets.push(pos + 1 + offset + i);
        text += child.text;
      } else {
        offsets.push(-1);
        text += "\u0000";
      }
    });
    const lower = text.toLowerCase();
    let index = lower.indexOf(q);
    while (index >= 0) {
      const from = offsets[index];
      const last = offsets[index + q.length - 1];
      if (from !== undefined && last !== undefined && from >= 0 && last >= 0) {
        matches.push({ from, to: last + 1 });
      }
      index = lower.indexOf(q, index + q.length);
    }
    return false;
  });
  return matches;
}

function build(doc: PMNode, query: string, current: number): FindState {
  const matches = findMatches(doc, query);
  const clamped = matches.length ? Math.min(Math.max(current, 0), matches.length - 1) : 0;
  const decorations = DecorationSet.create(
    doc,
    matches.map((m, i) =>
      Decoration.inline(m.from, m.to, {
        class: i === clamped ? "nt-find-match nt-find-current" : "nt-find-match",
      }),
    ),
  );
  return { query, matches, current: clamped, decorations };
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    findInNote: {
      setFindQuery: (query: string) => ReturnType;
      findNext: () => ReturnType;
      findPrevious: () => ReturnType;
      clearFind: () => ReturnType;
    };
  }
}

function step(direction: 1 | -1) {
  return ({
    state,
    dispatch,
    view,
  }: {
    state: import("@tiptap/pm/state").EditorState;
    dispatch?: (tr: import("@tiptap/pm/state").Transaction) => void;
    view: import("@tiptap/pm/view").EditorView;
  }) => {
    const find = findKey.getState(state);
    if (!find || find.matches.length === 0) return false;
    const current = (find.current + direction + find.matches.length) % find.matches.length;
    const match = find.matches[current];
    if (dispatch && match) {
      const tr = state.tr.setMeta(findKey, { current } satisfies Meta);
      tr.setSelection(TextSelection.create(tr.doc, match.from, match.to));
      dispatch(tr.scrollIntoView());
      // Keep the match comfortably in view inside the scrolling canvas.
      requestAnimationFrame(() => {
        // The note may have closed in the meantime.
        if (view.isDestroyed || match.from > view.state.doc.content.size) return;
        const dom = view.domAtPos(match.from).node as HTMLElement | Text;
        const el = dom instanceof Text ? dom.parentElement : dom;
        el?.scrollIntoView({ block: "center", behavior: "smooth" });
      });
    }
    return true;
  };
}

export const FindInNote = Extension.create({
  name: "findInNote",
  addCommands() {
    return {
      setFindQuery:
        (query: string) =>
        ({ tr, dispatch }) => {
          if (dispatch) dispatch(tr.setMeta(findKey, { query, current: 0 } satisfies Meta));
          return true;
        },
      findNext: () => (props) => step(1)(props),
      findPrevious: () => (props) => step(-1)(props),
      clearFind:
        () =>
        ({ tr, dispatch }) => {
          if (dispatch) dispatch(tr.setMeta(findKey, { query: "", current: 0 } satisfies Meta));
          return true;
        },
    };
  },
  addProseMirrorPlugins() {
    return [
      new Plugin<FindState>({
        key: findKey,
        state: {
          init: (_config, state) => build(state.doc, "", 0),
          apply: (tr, prev) => {
            const meta = tr.getMeta(findKey) as Meta | undefined;
            if (meta) {
              return build(tr.doc, meta.query ?? prev.query, meta.current ?? prev.current);
            }
            if (tr.docChanged && prev.query) return build(tr.doc, prev.query, prev.current);
            return prev;
          },
        },
        props: {
          decorations: (state) => findKey.getState(state)?.decorations,
        },
      }),
    ];
  },
});
