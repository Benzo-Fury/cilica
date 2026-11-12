/**
 * The list that opens when you type `[[` in a note: matching notes, saved people, and a way to
 * link a note that does not exist yet. ↑↓ to move, Enter or Tab to link, Esc to close.
 */
import type { Editor } from "@tiptap/core";
import type { EditorView } from "@tiptap/pm/view";
import { FilePlus2, FileText, User } from "lucide-react";
import { type RefObject, useImperativeHandle, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { suggestNotes, useLinkResolver } from "@/features/links/resolver";
import { displayTitle } from "@/lib/format";
import { linkKey } from "@/lib/links";
import { Icon } from "@/ui";
import type { SuggestHandlers, SuggestState } from "./editor/wikiSuggest";

interface Item {
  key: string;
  title: string;
  kind: "note" | "person" | "new";
  meta?: string;
}

export function LinkSuggest({
  editor,
  handlers,
  excludeNoteId,
}: {
  editor: Editor | null;
  handlers: RefObject<SuggestHandlers | null>;
  /** The note being edited: linking a note to itself is not offered. */
  excludeNoteId: string;
}) {
  const [open, setOpen] = useState<{ state: SuggestState; view: EditorView } | null>(null);
  const [active, setActive] = useState(0);
  const people = useLinkResolver((s) => s.people);
  const notes = useLinkResolver((s) => s.notes);

  const items = useMemo<Item[]>(() => {
    if (!open) return [];
    const query = open.state.query.trim();
    const list: Item[] = suggestNotes(query, 8)
      .filter((n) => n.id !== excludeNoteId)
      .map((n) => ({ key: `n:${n.id}`, title: n.title, kind: "note", meta: n.snippet }));
    const q = linkKey(query);
    if (q) {
      for (const p of people.values()) {
        if (list.length >= 9) break;
        if (linkKey(p.name).includes(q))
          list.push({ key: `p:${p.id}`, title: p.name, kind: "person" });
      }
      const exact = notes.has(q) || people.has(q);
      if (!exact) list.push({ key: "new", title: query, kind: "new" });
    }
    return list;
    // `notes` and `people` are read so the list follows edits made while it is open.
  }, [open, excludeNoteId, people, notes]);

  const pick = (item: Item | undefined) => {
    if (!editor || !open || !item) return;
    editor
      .chain()
      .focus()
      .insertWikiLink(
        { from: open.state.from, to: open.state.to },
        { target: item.title, alias: null, embed: open.state.embed },
      )
      .run();
    setOpen(null);
  };

  useImperativeHandle(
    handlers,
    (): SuggestHandlers => ({
      onChange: (state, view) => {
        setOpen(state ? { state, view } : null);
        setActive(0);
      },
      onKey: (key) => {
        if (!open) return false;
        if (key === "Escape") {
          editor?.commands.dismissWikiSuggest();
          return true;
        }
        if (!items.length) return false;
        if (key === "ArrowDown") setActive((a) => (a + 1) % items.length);
        else if (key === "ArrowUp") setActive((a) => (a - 1 + items.length) % items.length);
        else pick(items[Math.min(active, items.length - 1)]);
        return true;
      },
    }),
  );

  if (!open || !items.length) return null;
  let coords: { left: number; bottom: number } | null = null;
  try {
    coords = open.view.coordsAtPos(open.state.from);
  } catch {
    return null;
  }
  const style = {
    left: Math.min(coords.left, window.innerWidth - 336),
    top: coords.bottom + 6,
  };
  return createPortal(
    <div
      className="nt-menu nt-glass-strong nt-menu-layer nt-link-suggest"
      role="listbox"
      aria-label="Link to a note"
      style={style}
      // Keep the caret in the editor while choosing with the mouse.
      onMouseDown={(e) => e.preventDefault()}
    >
      {items.map((item, i) => (
        <div
          key={item.key}
          role="option"
          aria-selected={i === active}
          tabIndex={-1}
          className="nt-menu-item nt-link-suggest-item"
          data-highlighted={i === active ? "" : undefined}
          onMouseEnter={() => setActive(i)}
          onClick={() => pick(item)}
          onKeyDown={() => undefined}
        >
          <Icon
            icon={item.kind === "person" ? User : item.kind === "new" ? FilePlus2 : FileText}
            size="sm"
          />
          <span className="nt-link-suggest-text">
            <span className="nt-link-suggest-title">
              {item.kind === "new" ? `Link “${item.title}”` : displayTitle(item.title)}
            </span>
            {item.kind === "new" && <span className="nt-link-suggest-meta">Not created yet</span>}
            {item.kind === "person" && <span className="nt-link-suggest-meta">Saved person</span>}
            {item.kind === "note" && item.meta && (
              <span className="nt-link-suggest-meta">{item.meta}</span>
            )}
          </span>
        </div>
      ))}
    </div>,
    document.body,
  );
}
