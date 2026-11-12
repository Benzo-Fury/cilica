import { DndContext, DragOverlay, useDroppable } from "@dnd-kit/core";
import { motion, useReducedMotion } from "motion/react";
import { useCallback, useMemo } from "react";
import { reducedTransition, springs } from "@/app/motion";
import { useRouter } from "@/app/router";
import {
  groupNotes,
  useCategories,
  useCreateCategory,
  useMoveNote,
  useNotes,
} from "@/features/notes/hooks";
import { positionForInsert } from "@/features/notes/positions";
import { isLive, useRecordingState } from "@/features/recorder/hooks";
import type { NoteSummary } from "@/ipc/types";
import { displayTitle } from "@/lib/format";
import { cn, Row, Skeleton } from "@/ui";
import { CategoryBlock } from "./CategoryBlock";
import { InlineName } from "./InlineName";
import { NoteRow } from "./NoteRow";
import { useSidebarUi } from "./store";
import { type DragData, useSidebarDnd } from "./useSidebarDnd";

/** Shown at the top while a filed note is dragged: drop here to take it out of its category. */
function UncategorisedDrop({ target }: { target: boolean }) {
  const { setNodeRef } = useDroppable({ id: "section:notes", data: { type: "section" } });
  return (
    <div ref={setNodeRef} className={cn("nt-sidebar-dropzone", target && "nt-drop-target")}>
      Drop here to take it out of its category
    </div>
  );
}

function NewCategoryRow() {
  const create = useCreateCategory();
  const stop = useSidebarUi((s) => s.stopNewCategory);
  return (
    <Row
      className="nt-row-heading nt-row-editing"
      label={
        <InlineName
          initial=""
          placeholder="Category name"
          label="New category name"
          onCommit={(name) => {
            create.mutate(name);
            stop();
          }}
          onCancel={stop}
        />
      }
    />
  );
}

function DragPreview({
  data,
  notes,
  categoryName,
}: {
  data: DragData;
  notes: readonly NoteSummary[];
  categoryName?: string;
}) {
  const reduced = useReducedMotion();
  const note = data.type === "note" ? notes.find((n) => n.id === data.noteId) : undefined;
  return (
    <motion.div
      className="nt-row nt-row-dragging nt-glass-strong"
      initial={{ scale: 1, rotate: 0 }}
      animate={reduced ? { scale: 1 } : { scale: 1.02, rotate: -0.6 }}
      transition={reduced ? reducedTransition : springs.snappy}
    >
      {data.type === "note" ? (
        <span className="nt-row-label">{displayTitle(note?.title)}</span>
      ) : (
        <span className="nt-row-label">{categoryName}</span>
      )}
    </motion.div>
  );
}

/** Uncategorised notes first with no heading, then each category as a heading with its notes. Drag to file or reorder. */
export function NoteTree() {
  const { data: notes = [], isLoading: notesLoading } = useNotes();
  const { data: categories = [], isLoading: categoriesLoading } = useCategories();
  const grouped = useMemo(() => groupNotes(notes, categories), [notes, categories]);
  const currentNoteId = useRouter((s) => (s.view.name === "note" ? s.view.noteId : null));
  const recording = useRecordingState();
  const recordingNoteId = isLive(recording) ? recording.note_id : null;
  const newCategory = useSidebarUi((s) => s.newCategory);
  const move = useMoveNote();
  const dnd = useSidebarDnd(grouped, categories);

  const onNudge = useCallback(
    (note: NoteSummary, direction: -1 | 1) => {
      const list = note.category_id
        ? (grouped.byCategory.get(note.category_id) ?? [])
        : grouped.uncategorised;
      const index = list.findIndex((n) => n.id === note.id);
      const target = direction === -1 ? index - 1 : index + 2;
      if (target < 0 || target > list.length) return;
      const position = positionForInsert(list, note.id, target);
      if (position !== null)
        move.mutate({ noteId: note.id, categoryId: note.category_id, position });
    },
    [grouped, move],
  );

  const drop = dnd.drop;
  const draggingNote = dnd.active?.type === "note" ? dnd.active.noteId : null;
  const draggingCategory = dnd.active?.type === "category" ? dnd.active.categoryId : null;
  const lineFor = (noteId: string) =>
    drop?.kind === "note-line" && drop.overId === noteId ? drop.edge : null;

  const renderNote = (note: NoteSummary) => (
    <NoteRow
      key={note.id}
      note={note}
      current={note.id === currentNoteId}
      recording={note.id === recordingNoteId}
      categories={categories}
      dropLine={lineFor(note.id)}
      dragging={note.id === draggingNote}
      droppable={dnd.active?.type !== "category"}
      onNudge={onNudge}
    />
  );

  if (notesLoading || categoriesLoading) {
    return (
      <div className="nt-sidebar-loading" aria-busy>
        <Skeleton />
        <Skeleton />
        <Skeleton className="nt-skel-short" />
      </div>
    );
  }

  return (
    <DndContext
      sensors={dnd.sensors}
      collisionDetection={dnd.collision}
      onDragStart={dnd.onDragStart}
      onDragMove={dnd.onDragMove}
      onDragEnd={dnd.onDragEnd}
      onDragCancel={dnd.onDragCancel}
    >
      {dnd.active?.type === "note" && dnd.active.categoryId !== null && (
        <UncategorisedDrop target={drop?.kind === "note-into" && drop.categoryId === null} />
      )}
      {notes.length === 0 && <div className="nt-sidebar-hint">No notes yet</div>}
      {grouped.uncategorised.map((note) => renderNote(note))}
      {newCategory && <NewCategoryRow />}
      {categories.map((category) => {
        const list = grouped.byCategory.get(category.id) ?? [];
        return (
          <CategoryBlock
            key={category.id}
            category={category}
            count={list.length}
            dropTarget={drop?.kind === "note-into" && drop.categoryId === category.id}
            dropLine={drop?.kind === "cat-line" && drop.overId === category.id ? drop.edge : null}
            dragging={category.id === draggingCategory}
          >
            {list.map((note) => renderNote(note))}
          </CategoryBlock>
        );
      })}
      <DragOverlay dropAnimation={null}>
        {dnd.active && (
          <DragPreview
            data={dnd.active}
            notes={notes}
            categoryName={categories.find((c) => c.id === draggingCategory)?.name}
          />
        )}
      </DragOverlay>
    </DndContext>
  );
}
