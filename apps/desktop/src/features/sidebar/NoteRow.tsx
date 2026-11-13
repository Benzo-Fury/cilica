import { useDraggable, useDroppable } from "@dnd-kit/core";
import { Folder, FolderInput, Inbox, Mic, PenLine, Sparkles, Trash2 } from "lucide-react";
import { type KeyboardEvent, type MouseEvent, memo, useCallback } from "react";
import { useShortcut } from "@/app/keybinds";
import { isMac } from "@/app/platform";
import { openNote } from "@/app/router";
import { useMoveNote, useRenameNote, useTrashNote } from "@/features/notes/hooks";
import { type ClickKind, clickKind, countNotes } from "@/features/notes/selection";
import type { Category, NoteSummary } from "@/ipc/types";
import { displayTitle, relativeDate } from "@/lib/format";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuTrigger,
  cn,
  Icon,
  Row,
  Tooltip,
} from "@/ui";
import { InlineName } from "./InlineName";
import { focusSibling, SIDEBAR_PILL } from "./rows";
import { useSidebarUi } from "./store";
import { useBulkActions } from "./useBulkActions";
import type { DragData, DropData } from "./useSidebarDnd";

export interface NoteRowProps {
  note: NoteSummary;
  current: boolean;
  recording: boolean;
  categories: readonly Category[];
  dropLine: "before" | "after" | null;
  dragging: boolean;
  droppable: boolean;
  /** In the selection: highlighted. */
  selected: boolean;
  /** In a selection of several: the context menu acts on all of them. */
  groupMenu: boolean;
  /** Alt+↑/↓: move one step within its list. */
  onNudge: (note: NoteSummary, direction: -1 | 1) => void;
  /** A click (plain, ⌘ or Shift) or Enter on the row. */
  onSelect: (note: NoteSummary, kind: ClickKind) => void;
  /** Shift+↑/↓: grow or shrink the selection. */
  onExtend: (note: NoteSummary, direction: -1 | 1) => void;
  /** ⌘A: select every note shown. */
  onSelectAll: () => void;
  /** Esc: clear the selection. Returns whether there was anything to clear. */
  onClearSelection: () => boolean;
}

/** The context menu for a row that is part of a selection of several notes. */
function BulkMenuItems({ categories }: { categories: readonly Category[] }) {
  const bulk = useBulkActions();
  const trashKeys = useShortcut("note.trash");
  const n = countNotes(bulk.count);
  return (
    <>
      <ContextMenuSub label={`Move ${n} to`} icon={FolderInput} iconMotion="arrow">
        <ContextMenuItem
          icon={Inbox}
          disabled={bulk.sharedCategory === null}
          onSelect={() => bulk.moveTo(null, "Notes")}
        >
          Notes
        </ContextMenuItem>
        {categories.map((c) => (
          <ContextMenuItem
            key={c.id}
            icon={Folder}
            iconMotion="folder"
            disabled={bulk.sharedCategory === c.id}
            onSelect={() => bulk.moveTo(c.id, c.name)}
          >
            {c.name}
          </ContextMenuItem>
        ))}
      </ContextMenuSub>
      <ContextMenuItem icon={Sparkles} onSelect={bulk.ask}>
        Ask Cilica about {n}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem
        icon={Trash2}
        iconMotion="trash"
        danger
        shortcut={trashKeys}
        onSelect={bulk.trash}
      >
        Move {n} to Trash
      </ContextMenuItem>
    </>
  );
}

export const NoteRow = memo(function NoteRow({
  note,
  current,
  recording,
  categories,
  dropLine,
  dragging,
  droppable,
  selected,
  groupMenu,
  onNudge,
  onSelect,
  onExtend,
  onSelectAll,
  onClearSelection,
}: NoteRowProps) {
  const renaming = useSidebarUi((s) => s.renaming?.kind === "note" && s.renaming.id === note.id);
  const setRenaming = useSidebarUi((s) => s.setRenaming);
  const rename = useRenameNote();
  const move = useMoveNote();
  const trash = useTrashNote();
  const trashKeys = useShortcut("note.trash");

  const dragData: DragData = { type: "note", noteId: note.id, categoryId: note.category_id };
  const dropData: DropData = { type: "note", noteId: note.id, categoryId: note.category_id };
  const draggable = useDraggable({ id: `note:${note.id}`, data: dragData, disabled: renaming });
  const drop = useDroppable({ id: `note-drop:${note.id}`, data: dropData, disabled: !droppable });
  const setRef = useCallback(
    (el: HTMLDivElement | null) => {
      draggable.setNodeRef(el);
      drop.setNodeRef(el);
    },
    [draggable.setNodeRef, drop.setNodeRef],
  );

  const { role: _role, tabIndex: _tabIndex, ...dragAttributes } = draggable.attributes;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (renaming) return;
    const mod = isMac ? event.metaKey : event.ctrlKey;
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onSelect(note, "plain");
      openNote(note.id);
    } else if (mod && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "a") {
      event.preventDefault();
      onSelectAll();
    } else if (event.key === "Escape") {
      if (onClearSelection()) event.preventDefault();
    } else if (event.shiftKey && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
      event.preventDefault();
      onExtend(note, event.key === "ArrowUp" ? -1 : 1);
    } else if (event.key === "F2") {
      event.preventDefault();
      setRenaming({ kind: "note", id: note.id });
    } else if (event.altKey && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
      event.preventDefault();
      onNudge(note, event.key === "ArrowUp" ? -1 : 1);
    } else if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      focusSibling(event.currentTarget, event.key === "ArrowUp" ? -1 : 1);
    }
  };

  // The date shows only on the row you point at (see `.nt-note-row` in ui.css).
  const meta = <span>{relativeDate(note.updated_at)}</span>;

  const onClick = (event: MouseEvent<HTMLDivElement>) => {
    if (renaming) return;
    const kind = clickKind(event, isMac);
    onSelect(note, kind);
    if (kind === "plain") openNote(note.id);
  };

  const row = (
    <Row
      {...dragAttributes}
      {...draggable.listeners}
      ref={setRef}
      data-nav-row
      data-note-id={note.id}
      role="button"
      tabIndex={0}
      aria-label={`${displayTitle(note.title)}${note.kind === "meeting" ? ", meeting" : ""}${recording ? ", recording" : ""}${selected ? ", selected" : ""}`}
      current={current}
      selected={selected}
      pillId={SIDEBAR_PILL}
      dimmed={dragging}
      dropLine={dropLine}
      className={cn("nt-note-row", renaming && "nt-row-editing")}
      trailing={recording ? <Icon icon={Mic} size="sm" className="nt-note-rec" /> : undefined}
      label={
        renaming ? (
          <InlineName
            initial={note.title}
            placeholder="Untitled"
            label="Note title"
            onCommit={(title) => {
              rename.mutate({ noteId: note.id, title });
              setRenaming(null);
            }}
            onCancel={() => setRenaming(null)}
          />
        ) : (
          displayTitle(note.title)
        )
      }
      meta={renaming ? undefined : meta}
      onClick={onClick}
      // Shift-click selects notes, not the text of the rows in between.
      onMouseDown={(event) => event.shiftKey && event.preventDefault()}
      onDoubleClick={() => setRenaming({ kind: "note", id: note.id })}
      onKeyDown={onKeyDown}
    />
  );

  return (
    <div>
      <ContextMenu>
        <Tooltip
          label={
            <span className="nt-tip-preview">
              <b>{displayTitle(note.title)}</b>
              {note.snippet && <span>{note.snippet}</span>}
            </span>
          }
          side="right"
          delay={900}
          enabled={!renaming && !dragging && Boolean(note.snippet)}
        >
          <ContextMenuTrigger asChild>{row}</ContextMenuTrigger>
        </Tooltip>
        <ContextMenuContent>
          {groupMenu ? (
            <BulkMenuItems categories={categories} />
          ) : (
            <>
              <ContextMenuItem
                icon={PenLine}
                onSelect={() => setRenaming({ kind: "note", id: note.id })}
                shortcut={["F2"]}
              >
                Rename
              </ContextMenuItem>
              <ContextMenuSub label="Move to" icon={FolderInput} iconMotion="arrow">
                <ContextMenuItem
                  icon={Inbox}
                  disabled={note.category_id === null}
                  onSelect={() =>
                    move.mutate({
                      noteId: note.id,
                      categoryId: null,
                      position: null,
                      announce: "Notes",
                    })
                  }
                >
                  Notes
                </ContextMenuItem>
                {categories.map((c) => (
                  <ContextMenuItem
                    key={c.id}
                    icon={Folder}
                    iconMotion="folder"
                    disabled={note.category_id === c.id}
                    onSelect={() =>
                      move.mutate({
                        noteId: note.id,
                        categoryId: c.id,
                        position: null,
                        announce: c.name,
                      })
                    }
                  >
                    {c.name}
                  </ContextMenuItem>
                ))}
              </ContextMenuSub>
              <ContextMenuSeparator />
              <ContextMenuItem
                icon={Trash2}
                iconMotion="trash"
                danger
                shortcut={trashKeys}
                onSelect={() => trash.mutate(note.id)}
              >
                Move to Trash
              </ContextMenuItem>
            </>
          )}
        </ContextMenuContent>
      </ContextMenu>
    </div>
  );
});
