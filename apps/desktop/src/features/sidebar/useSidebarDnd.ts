/**
 * Drag and drop for the sidebar (dnd-kit): notes move between Notes and categories or reorder
 * inside one; categories reorder. A drop line marks the insert point; hovering a collapsed
 * category for 500ms opens it. Positions are fractional, so only the moved item is written,
 * optimistically, with rollback on failure.
 */
import {
  type CollisionDetection,
  closestCenter,
  type DragEndEvent,
  type DragMoveEvent,
  type DragStartEvent,
  PointerSensor,
  pointerWithin,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { useMoveNote, useReorderCategory, useSetCategoryCollapsed } from "@/features/notes/hooks";
import { positionForInsert } from "@/features/notes/positions";
import type { Category, NoteSummary } from "@/ipc/types";
import { haptic } from "@/lib/haptics";

export type DragData =
  | { type: "note"; noteId: string; categoryId: string | null }
  | { type: "category"; categoryId: string };

export type DropData =
  | { type: "note"; noteId: string; categoryId: string | null }
  | { type: "category"; categoryId: string }
  | { type: "section" };

export type DropState =
  | {
      kind: "note-line";
      bucket: string | null;
      overId: string;
      edge: "before" | "after";
      index: number;
    }
  | { kind: "note-into"; categoryId: string | null }
  | { kind: "cat-line"; overId: string; edge: "before" | "after"; index: number }
  | null;

export const dragId = (data: DragData) =>
  data.type === "note" ? `note:${data.noteId}` : `cat:${data.categoryId}`;

const collision: CollisionDetection = (args) => {
  const within = pointerWithin(args);
  return within.length > 0 ? within : closestCenter(args);
};

interface Grouped {
  uncategorised: NoteSummary[];
  byCategory: Map<string, NoteSummary[]>;
}

export function useSidebarDnd(grouped: Grouped, categories: readonly Category[]) {
  const [active, setActiveState] = useState<DragData | null>(null);
  const [drop, setDropState] = useState<DropState>(null);
  // Mirrors of the state for the end handler, which may run before a re-render.
  const activeRef = useRef<DragData | null>(null);
  const dropRef = useRef<DropState>(null);
  const setActive = (value: DragData | null) => {
    activeRef.current = value;
    setActiveState(value);
  };
  const setDrop = (value: DropState) => {
    const prev = dropRef.current;
    if (JSON.stringify(prev) === JSON.stringify(value)) return;
    dropRef.current = value;
    setDropState(value);
  };
  const moveNote = useMoveNote();
  const reorderCategory = useReorderCategory();
  const setCollapsed = useSetCategoryCollapsed();
  const expandTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const expandTarget = useRef<string | null>(null);

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 4 } }));

  const clearExpand = useCallback(() => {
    if (expandTimer.current) clearTimeout(expandTimer.current);
    expandTimer.current = null;
    expandTarget.current = null;
  }, []);
  useEffect(() => clearExpand, [clearExpand]);

  const bucketOf = (categoryId: string | null) =>
    categoryId ? (grouped.byCategory.get(categoryId) ?? []) : grouped.uncategorised;

  const onDragStart = (event: DragStartEvent) => {
    setActive((event.active.data.current as DragData | undefined) ?? null);
    setDrop(null);
  };

  const onDragMove = (event: DragMoveEvent | DragEndEvent) => {
    const data = event.active.data.current as DragData | undefined;
    const over = event.over;
    if (!data || !over) {
      clearExpand();
      setDrop(null);
      return;
    }
    const target = over.data.current as DropData | undefined;
    const start = event.activatorEvent as PointerEvent | MouseEvent;
    const pointerY = ("clientY" in start ? start.clientY : 0) + event.delta.y;
    const after = pointerY > over.rect.top + over.rect.height / 2;
    const edge = after ? "after" : "before";

    if (data.type === "note") {
      if (target?.type === "note") {
        clearExpand();
        const list = bucketOf(target.categoryId);
        const i = list.findIndex((n) => n.id === target.noteId);
        setDrop({
          kind: "note-line",
          bucket: target.categoryId,
          overId: target.noteId,
          edge,
          index: after ? i + 1 : i,
        });
      } else if (target?.type === "category") {
        setDrop({ kind: "note-into", categoryId: target.categoryId });
        const category = categories.find((c) => c.id === target.categoryId);
        if (category?.collapsed && expandTarget.current !== category.id) {
          clearExpand();
          expandTarget.current = category.id;
          expandTimer.current = setTimeout(() => {
            setCollapsed.mutate({ categoryId: category.id, collapsed: false });
          }, 500);
        }
      } else if (target?.type === "section") {
        clearExpand();
        setDrop({ kind: "note-into", categoryId: null });
      }
      return;
    }

    if (data.type === "category" && target?.type === "category") {
      const i = categories.findIndex((c) => c.id === target.categoryId);
      setDrop({ kind: "cat-line", overId: target.categoryId, edge, index: after ? i + 1 : i });
    }
  };

  const reset = () => {
    clearExpand();
    setActive(null);
    setDrop(null);
  };

  const onDragEnd = (event: DragEndEvent) => {
    // A very quick drag may end without a move event: settle the target from the end event.
    onDragMove(event);
    const data = activeRef.current;
    const state = dropRef.current;
    reset();
    if (!data || !state) return;

    if (data.type === "note" && (state.kind === "note-line" || state.kind === "note-into")) {
      const bucket = state.kind === "note-line" ? state.bucket : state.categoryId;
      const index = state.kind === "note-line" ? state.index : 0;
      const list = bucketOf(bucket);
      const sameBucket = bucket === data.categoryId;
      const position = positionForInsert(list, data.noteId, index);
      if (sameBucket && position === null) return;
      const name = bucket ? categories.find((c) => c.id === bucket)?.name : "Notes";
      haptic("alignment");
      moveNote.mutate({
        noteId: data.noteId,
        categoryId: bucket,
        position: position ?? 0,
        announce: sameBucket ? undefined : name,
      });
      return;
    }

    if (data.type === "category" && state.kind === "cat-line") {
      const position = positionForInsert(categories, data.categoryId, state.index);
      if (position === null) return;
      haptic("alignment");
      reorderCategory.mutate({ categoryId: data.categoryId, position });
    }
  };

  return {
    sensors,
    collision,
    active,
    drop,
    onDragStart,
    onDragMove,
    onDragEnd,
    onDragCancel: reset,
  };
}
