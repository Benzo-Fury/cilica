/**
 * An image node in the note editor: the ImageBlock with its source resolved, sizes and captions
 * written back to the node, the grip wired to drag-to-move, the viewer on double-click, and a
 * context menu with the same actions as the image toolbar.
 */
import { type NodeViewProps, NodeViewWrapper, ReactNodeViewRenderer } from "@tiptap/react";
import {
  CloudDownload,
  Copy,
  Download,
  FolderOpen,
  Maximize2,
  Replace,
  TextCursorInput,
  Trash,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { isAttachmentPath } from "@/ipc/attachments";
import { isTauri } from "@/ipc/client";
import {
  Button,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
  ImageBlock,
} from "@/ui";
import { type ImageAttrs, MIN_IMAGE_WIDTH, moveSelectedImage } from "../editor/image";
import { imageActions, revealLabel } from "./actions";
import { startImageDrag } from "./dragMove";
import { useImageUi } from "./store";
import { useImageSrc } from "./useImageSrc";

function ImageView({ editor, node, getPos, selected, updateAttributes }: NodeViewProps) {
  const attrs = node.attrs as ImageAttrs;
  const source = useImageSrc(attrs.src);
  const actions = useMemo(() => imageActions(editor), [editor]);
  const pos = () => (typeof getPos === "function" ? getPos() : undefined);
  const [dragging, setDragging] = useState(false);
  const [settling, setSettling] = useState(() =>
    attrs.src ? useImageUi.getState().takeSettling(attrs.src) : false,
  );
  useEffect(() => {
    if (!settling) return;
    const t = setTimeout(() => setSettling(false), 720);
    return () => clearTimeout(t);
  }, [settling]);

  // Selected in the document is not enough: it must also be where focus is (see `activeImage`).
  const activeImage = useImageUi((s) => s.activeImage);
  const active = selected && activeImage !== null && activeImage === pos();
  const captionAt = useImageUi((s) => s.captionAt);
  const editingCaption = captionAt !== null && captionAt === pos();

  const editable = editor.isEditable;
  const at = () => {
    const p = pos();
    return p === undefined ? null : p;
  };
  const run = (fn: (p: number) => void) => () => {
    const p = at();
    if (p !== null) fn(p);
  };

  const isFile = isAttachmentPath(attrs.src);
  const ready = source.kind === "ready";
  const missing =
    source.kind === "web"
      ? {
          title: "Web image not shown",
          detail:
            "Cilica doesn't load images from the web, so sites can't see when you open this note.",
          actions: editable ? (
            <Button
              size="sm"
              icon={CloudDownload}
              onClick={run(actions.saveWebCopy)}
              loading={!!attrs.pending}
            >
              Save a copy
            </Button>
          ) : null,
        }
      : {
          title: "Image missing",
          detail: attrs.src?.startsWith("attachments/")
            ? "Its file is no longer in Cilica's attachments folder."
            : `Cilica can't find ${attrs.src ? `“${attrs.src.split("/").pop()}”` : "this image"}.`,
          actions: editable ? (
            <>
              <Button size="sm" onClick={run(actions.replace)}>
                Replace…
              </Button>
              <Button size="sm" variant="ghost" onClick={run(actions.remove)}>
                Remove
              </Button>
            </>
          ) : null,
        };

  const block = (
    <ImageBlock
      src={ready ? source.url : null}
      alt={attrs.alt ?? ""}
      caption={attrs.title ?? ""}
      width={attrs.width}
      align={attrs.align ?? "left"}
      selected={active}
      busy={!!attrs.pending}
      settling={settling}
      dragging={dragging}
      editable={editable}
      editingCaption={editingCaption}
      minWidth={MIN_IMAGE_WIDTH}
      missing={
        (source.kind === "web" || source.kind === "missing") && !(attrs.pending && !attrs.src)
          ? missing
          : undefined
      }
      onResizeEnd={(width) => updateAttributes({ width })}
      onCaptionCommit={(title) => updateAttributes({ title: title || null })}
      onCaptionDone={() => {
        if (useImageUi.getState().captionAt !== null) useImageUi.getState().editCaption(null);
      }}
      onOpen={run(actions.view)}
      onGripPointerDown={(e) => {
        const p = at();
        if (p !== null) startImageDrag(editor, p, attrs.src, e.nativeEvent, setDragging);
      }}
      onGripKeyDown={(e) => {
        if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
        e.preventDefault();
        const p = at();
        if (p === null) return;
        actions.select(p);
        moveSelectedImage(editor.state, e.key === "ArrowUp" ? -1 : 1, editor.view.dispatch);
      }}
    />
  );

  return (
    <NodeViewWrapper className="nt-image-node" data-drag-handle={undefined} contentEditable={false}>
      <ContextMenu
        onOpenChange={(open) => {
          if (open) run(actions.select)();
        }}
      >
        <ContextMenuTrigger asChild>
          <div>{block}</div>
        </ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem icon={Maximize2} disabled={!ready} onSelect={run(actions.view)}>
            View image
          </ContextMenuItem>
          <ContextMenuItem icon={Copy} disabled={!ready} onSelect={run(actions.copy)}>
            Copy image
          </ContextMenuItem>
          <ContextMenuItem
            icon={Download}
            disabled={!ready || !isFile}
            onSelect={run(actions.saveAs)}
          >
            Save image as…
          </ContextMenuItem>
          {isTauri && (
            <ContextMenuItem
              icon={FolderOpen}
              disabled={!ready || !isFile}
              onSelect={run(actions.reveal)}
            >
              {revealLabel}
            </ContextMenuItem>
          )}
          {editable && (
            <>
              <ContextMenuSeparator />
              {source.kind === "web" && (
                <ContextMenuItem icon={CloudDownload} onSelect={run(actions.saveWebCopy)}>
                  Save a copy
                </ContextMenuItem>
              )}
              <ContextMenuItem icon={Replace} onSelect={run(actions.replace)}>
                Replace…
              </ContextMenuItem>
              <ContextMenuItem
                icon={TextCursorInput}
                onSelect={() => {
                  run(actions.select)();
                  useImageUi.getState().setEditingAlt(true);
                }}
              >
                Edit alt text…
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem icon={Trash} danger onSelect={run(actions.remove)}>
                Delete image
              </ContextMenuItem>
            </>
          )}
        </ContextMenuContent>
      </ContextMenu>
    </NodeViewWrapper>
  );
}

/** The node view the note editor gives its image node. */
export const ImageNodeView = ReactNodeViewRenderer(ImageView);
