/**
 * Offering to link a note title you just typed. `useInlineLinkMatcher` gives the editor its matcher
 * (titles from the link resolver, minus this note, the titles it turned down, what it already
 * links and titles you keep ignoring); `InlineLinkHint` draws the pill under the words and
 * remembers how you answered it (`hintMemory.ts`).
 */
import type { Editor } from "@tiptap/core";
import type { EditorView } from "@tiptap/pm/view";
import { type RefObject, useEffect, useImperativeHandle, useRef, useState } from "react";
import { createPortal, flushSync } from "react-dom";
import { isQuiet, isTrusted, useHintMemory } from "@/features/links/hintMemory";
import { buildTitleIndex, matchTitleBefore, type TitleIndex } from "@/features/links/inlineLink";
import { useLinkResolver } from "@/features/links/resolver";
import type { Note } from "@/ipc/types";
import { displayTitle } from "@/lib/format";
import { linkKey } from "@/lib/links";
import { linkTarget, listProperty, splitProperties, UNRELATED } from "@/lib/properties";
import { LinkHint } from "@/ui";
import type {
  InlineLinkHint as Hint,
  InlineLinkHandlers,
  InlineLinkMatcher,
} from "./editor/inlineLinkHint";

/** The matcher for one note. Reads the latest note and titles on every call, so it never goes stale. */
export function useInlineLinkMatcher(note: Note): InlineLinkMatcher {
  const latest = useRef(note);
  latest.current = note;
  const cache = useRef<{ from: unknown; index: TitleIndex } | null>(null);
  const matcher = useRef<InlineLinkMatcher | null>(null);
  matcher.current ??= (before, isLinked) => {
    // The index is rebuilt only when the notes list changes, not per word.
    const notes = useLinkResolver.getState().notes;
    if (cache.current?.from !== notes)
      cache.current = { from: notes, index: buildTitleIndex(notes) };
    const { id, body_md } = latest.current;
    const memory = useHintMemory.getState().titles;
    return matchTitleBefore(before, cache.current.index, {
      skip: (key, entry) => {
        if (entry.noteId === id || isLinked(key) || isQuiet(memory[key])) return true;
        const { yaml } = splitProperties(body_md);
        return yaml
          ? listProperty(yaml, UNRELATED).some((v) => linkKey(linkTarget(v)) === key)
          : false;
      },
      trusted: (key) => isTrusted(memory[key]),
    });
  };
  return matcher.current;
}

export function InlineLinkHint({
  editor,
  handlers,
}: {
  editor: Editor | null;
  handlers: RefObject<InlineLinkHandlers | null>;
}) {
  const [open, setOpen] = useState<{ hint: Hint; view: EditorView } | null>(null);
  useImperativeHandle(
    handlers,
    (): InlineLinkHandlers => ({
      onChange: (hint, view) => {
        // Showing only ever follows a key you typed: draw it now, in the same frame as the
        // key, rather than a frame later. Hiding can wait.
        if (hint) flushSync(() => setOpen({ hint, view }));
        else setOpen(null);
      },
      onOutcome: (hint, outcome) => useHintMemory.getState().record(hint.key, outcome),
    }),
  );
  // It sits where the words were: scrolling them away puts it away too.
  useEffect(() => {
    if (!open || !editor) return;
    const away = () => editor.commands.dismissInlineLink();
    window.addEventListener("scroll", away, { capture: true, passive: true, once: true });
    return () => window.removeEventListener("scroll", away, { capture: true });
  }, [open, editor]);
  if (!open || !editor) return null;
  let at: { left: number; bottom: number };
  try {
    at = open.view.coordsAtPos(open.hint.from);
  } catch {
    return null;
  }
  return createPortal(
    <LinkHint
      key={`${open.hint.from}:${open.hint.noteId}`}
      title={displayTitle(open.hint.title)}
      style={{ left: Math.max(8, Math.min(at.left, window.innerWidth - 328)), top: at.bottom + 4 }}
      onAccept={() => editor.chain().focus().acceptInlineLink().run()}
    />,
    document.body,
  );
}
