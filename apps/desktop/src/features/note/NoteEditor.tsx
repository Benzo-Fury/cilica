/**
 * The TipTap editor for a note's Markdown body. Saves are debounced through `useAutosave`;
 * refetches only touch the editor when the body really changed elsewhere (no cursor jumps);
 * `note_body_stream` events stream a meeting summary in while the editor is read-only.
 */
import { useQueryClient } from "@tanstack/react-query";
import type { Editor } from "@tiptap/core";
import { EditorContent, useEditor } from "@tiptap/react";
import { type Ref, useEffect, useImperativeHandle, useRef, useState } from "react";
import { useBackendEvent } from "@/app/backendEvents";
import { qk } from "@/app/queryKeys";
import { openGraph } from "@/app/router";
import { useLinkResolverSync, useOpenLink } from "@/features/links/hooks";
import { resolveLink, useLinkResolver } from "@/features/links/resolver";
import { useSwearTyping } from "@/features/swearjar/hooks";
import type { Note } from "@/ipc/types";
import { rangesOf, useAiGlow } from "./aiGlow";
import { noteExtensions } from "./editor/extensions";
import { normalizeMarkdown } from "./editor/markdown";
import type { SuggestHandlers } from "./editor/wikiSuggest";
import { LinkSuggest } from "./LinkSuggest";
import { useAutosave } from "./useAutosave";
import { useNoteViewState } from "./viewState";

export interface NoteEditorHandle {
  editor: Editor | null;
  flush: () => Promise<void>;
  /** The body as the editor has it now. */
  markdown: () => string;
  /** Replace the body (version restore); records it as saved. */
  replace: (markdown: string) => void;
  focusStart: () => void;
}

/** Replace the whole document without moving the caret more than needed. */
function setBody(editor: Editor, markdown: string) {
  const { from, to } = editor.state.selection;
  editor.commands.setContent(markdown, { contentType: "markdown", emitUpdate: false });
  const size = editor.state.doc.content.size;
  const clamp = (n: number) => Math.max(1, Math.min(n, size - 1));
  try {
    editor.commands.setTextSelection({ from: clamp(from), to: clamp(to) });
  } catch {
    // Selection outside the new document: leave ProseMirror's default.
  }
}

export function NoteEditor({
  note,
  onStreamingChange,
  ref,
}: {
  note: Note;
  onStreamingChange?: (streaming: boolean) => void;
  ref?: Ref<NoteEditorHandle>;
}) {
  const queryClient = useQueryClient();
  const autosave = useAutosave(note.id, note.body_md);
  const [streaming, setStreamingState] = useState(false);
  const streamingRef = useRef(false);
  const setStreaming = (value: boolean) => {
    streamingRef.current = value;
    setStreamingState(value);
    onStreamingChange?.(value);
  };

  const onWordEnd = useSwearTyping(note.id);
  useLinkResolverSync();
  const openLink = useOpenLink(note.category_id);
  const openLinkRef = useRef(openLink);
  openLinkRef.current = openLink;
  const suggest = useRef<SuggestHandlers | null>(null);
  const editor = useEditor({
    extensions: noteExtensions({
      placeholder: "Start writing",
      onWordEnd,
      links: {
        resolve: (target) => {
          const resolved = resolveLink(target);
          return {
            state: resolved.kind,
            preview: resolved.kind === "note" ? resolved.snippet : undefined,
          };
        },
        subscribe: (listener) => useLinkResolver.subscribe(listener),
        onOpen: (attrs) => openLinkRef.current(attrs.target),
      },
      suggest: () => suggest.current,
      onOpenTag: (tag) => openGraph(`t:${tag.toLowerCase()}`),
    }),
    content: note.body_md,
    contentType: "markdown",
    immediatelyRender: true,
    shouldRerenderOnTransaction: false,
    editorProps: {
      attributes: { "aria-label": "Note body", spellcheck: "true", class: "nt-selectable" },
    },
    onUpdate: ({ editor: e }) => autosave.schedule(e.getMarkdown()),
    // The assistant can see what is selected ("Viewing: … · 12 words selected").
    onSelectionUpdate: ({ editor: e }) => {
      const { from, to } = e.state.selection;
      useNoteViewState.getState().setSelection(note.id, e.state.doc.textBetween(from, to, "\n"));
    },
    onBlur: () => void autosave.flush(),
  });

  // A refetch landed: apply it only if it differs from what we know and we have no edits pending.
  useEffect(() => {
    if (!editor || streaming) return;
    const incoming = normalizeMarkdown(note.body_md);
    if (incoming === autosave.known()) return;
    if (autosave.isDirty()) return; // our pending save wins; it snapshots a version first
    if (incoming === normalizeMarkdown(editor.getMarkdown())) {
      autosave.setKnown(note.body_md);
      return;
    }
    autosave.setKnown(note.body_md);
    setBody(editor, note.body_md);
    // An accepted AI edit: wash the changed sentences, then let the wash fade.
    const phrases = useAiGlow.getState().take(note.id);
    if (phrases) {
      editor.commands.markRanges(rangesOf(editor.state.doc, phrases), "nt-ai-settle");
      const t = setTimeout(() => editor.isDestroyed || editor.commands.clearStreamed(), 2200);
      return () => clearTimeout(t);
    }
  }, [editor, note.id, note.body_md, streaming, autosave]);

  useBackendEvent("note_body_stream", (event) => {
    if (!editor || event.note_id !== note.id) return;
    if (!streamingRef.current) {
      setStreaming(true);
      editor.setEditable(false);
    }
    const before = editor.state.doc.content.size;
    editor.commands.setContent(event.body_md, { contentType: "markdown", emitUpdate: false });
    const after = editor.state.doc.content.size;
    autosave.setKnown(event.body_md);
    if (event.done) {
      editor.commands.markStreamed(1, after, "nt-ai-settle");
      editor.setEditable(true);
      setStreaming(false);
      void queryClient.invalidateQueries({ queryKey: qk.note(note.id) });
      void queryClient.invalidateQueries({ queryKey: qk.notes });
      setTimeout(() => editor.isDestroyed || editor.commands.clearStreamed(), 2200);
    } else if (after > before) {
      editor.commands.markStreamed(Math.max(1, before - 2), after, "nt-stream-new");
      // Follow the newest words, like a live transcript.
      const dom = editor.view.dom.lastElementChild;
      dom?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  });

  useImperativeHandle(
    ref,
    () => ({
      editor,
      flush: () => {
        if (editor && !streaming) autosave.schedule(editor.getMarkdown());
        return autosave.flush();
      },
      markdown: () => editor?.getMarkdown() ?? note.body_md,
      replace: (markdown: string) => {
        if (!editor) return;
        autosave.setKnown(markdown);
        setBody(editor, markdown);
      },
      focusStart: () => {
        if (!editor) return;
        // Synchronous focus (TipTap's focus command waits a frame).
        editor.commands.setTextSelection(1);
        editor.view.focus();
      },
    }),
    [editor, autosave, note.body_md, streaming],
  );

  return (
    <>
      <EditorContent editor={editor} className="nt-editor" />
      <LinkSuggest editor={editor} handlers={suggest} excludeNoteId={note.id} />
    </>
  );
}
