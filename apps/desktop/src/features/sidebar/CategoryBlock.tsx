import { useDraggable, useDroppable } from "@dnd-kit/core";
import { PenLine, Plus, Trash2 } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useCallback, useState } from "react";
import {
  useCreateNote,
  useDeleteCategory,
  useRenameCategory,
  useSetCategoryCollapsed,
} from "@/features/notes/hooks";
import type { Category } from "@/ipc/types";
import {
  Chevron,
  Collapse,
  ConfirmDialog,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
  cn,
  Row,
} from "@/ui";
import { InlineName } from "./InlineName";
import { focusSibling } from "./rows";
import { useSidebarUi } from "./store";
import type { DragData, DropData } from "./useSidebarDnd";

export interface CategoryBlockProps {
  category: Category;
  count: number;
  dropTarget: boolean;
  dropLine: "before" | "after" | null;
  dragging: boolean;
  children: ReactNode;
}

/** A category heading (small, with a chevron: click to fold, drag to reorder) and its notes. */
export function CategoryBlock({
  category,
  count,
  dropTarget,
  dropLine,
  dragging,
  children,
}: CategoryBlockProps) {
  const renaming = useSidebarUi(
    (s) => s.renaming?.kind === "category" && s.renaming.id === category.id,
  );
  const setRenaming = useSidebarUi((s) => s.setRenaming);
  const rename = useRenameCategory();
  const setCollapsed = useSetCategoryCollapsed();
  const remove = useDeleteCategory();
  const createNote = useCreateNote();
  const [confirming, setConfirming] = useState(false);
  const open = !category.collapsed;
  const panelId = `cat-${category.id}`;

  const dragData: DragData = { type: "category", categoryId: category.id };
  const dropData: DropData = { type: "category", categoryId: category.id };
  const draggable = useDraggable({ id: `cat:${category.id}`, data: dragData, disabled: renaming });
  const drop = useDroppable({ id: `cat-drop:${category.id}`, data: dropData });
  const setRef = useCallback(
    (el: HTMLDivElement | null) => {
      draggable.setNodeRef(el);
      drop.setNodeRef(el);
    },
    [draggable.setNodeRef, drop.setNodeRef],
  );
  const { role: _role, tabIndex: _tabIndex, ...dragAttributes } = draggable.attributes;

  const toggle = () => setCollapsed.mutate({ categoryId: category.id, collapsed: open });

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (renaming) return;
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      toggle();
    } else if (event.key === "ArrowRight" && !open) {
      event.preventDefault();
      toggle();
    } else if (event.key === "ArrowLeft" && open) {
      event.preventDefault();
      toggle();
    } else if (event.key === "F2") {
      event.preventDefault();
      setRenaming({ kind: "category", id: category.id });
    } else if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      focusSibling(event.currentTarget, event.key === "ArrowUp" ? -1 : 1);
    }
  };

  return (
    <div className="nt-sidebar-section">
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <Row
            {...dragAttributes}
            {...draggable.listeners}
            ref={setRef}
            data-nav-row
            role="button"
            tabIndex={0}
            aria-expanded={open}
            aria-controls={panelId}
            dropTarget={dropTarget}
            dropLine={dropLine}
            dimmed={dragging}
            className={cn("nt-row-heading", renaming && "nt-row-editing")}
            label={
              renaming ? (
                <InlineName
                  initial={category.name}
                  placeholder="Category name"
                  label="Category name"
                  onCommit={(name) => {
                    rename.mutate({ categoryId: category.id, name });
                    setRenaming(null);
                  }}
                  onCancel={() => setRenaming(null)}
                />
              ) : (
                <span className="nt-heading-label">
                  <span>{category.name}</span>
                  <Chevron />
                </span>
              )
            }
            onClick={() => !renaming && toggle()}
            onDoubleClick={(e) => {
              e.preventDefault();
              setRenaming({ kind: "category", id: category.id });
            }}
            onKeyDown={onKeyDown}
          />
        </ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem
            icon={Plus}
            iconMotion="plus"
            onSelect={() => createNote.mutate(category.id)}
          >
            New note in {category.name}
          </ContextMenuItem>
          <ContextMenuItem
            icon={PenLine}
            shortcut={["F2"]}
            onSelect={() => setRenaming({ kind: "category", id: category.id })}
          >
            Rename
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem
            icon={Trash2}
            iconMotion="trash"
            danger
            onSelect={() => setConfirming(true)}
          >
            Delete category
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
      <Collapse open={open} id={panelId}>
        <div className="nt-sidebar-section">{children}</div>
      </Collapse>
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title={`Delete ${category.name}?`}
        body={
          count > 0
            ? `Its ${count === 1 ? "note moves" : `${count} notes move`} to Notes. Nothing goes to Trash.`
            : "The category is empty, so nothing else changes."
        }
        confirmLabel="Delete category"
        destructive
        onConfirm={() => remove.mutate(category.id)}
      />
    </div>
  );
}
