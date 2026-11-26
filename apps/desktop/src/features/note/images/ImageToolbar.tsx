/**
 * The toolbar under a selected image: placement (left, centre, right), caption, alt text,
 * the viewer, and a menu with copy, save, show the file, replace and delete. It is the
 * selection toolbar's sibling: same glass bar, same placement, same Esc behaviour.
 */
import { type Editor, posToDOMRect } from "@tiptap/core";
import { NodeSelection } from "@tiptap/pm/state";
import { useEditorState } from "@tiptap/react";
import {
  AlignHorizontalJustifyCenter,
  AlignHorizontalJustifyEnd,
  AlignHorizontalJustifyStart,
  Captions,
  Check,
  Copy,
  Download,
  Ellipsis,
  FolderOpen,
  Maximize2,
  Replace,
  TextCursorInput,
  Trash,
} from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useEscapeLayer } from "@/app/layers";
import { isAttachmentPath } from "@/ipc/attachments";
import { isTauri } from "@/ipc/client";
import {
  FormatBar,
  FormatBarDivider,
  FormatBarField,
  IconButton,
  Menu,
  MenuContent,
  MenuItem,
  MenuSeparator,
  MenuTrigger,
} from "@/ui";
import { placeToolbar } from "../editor/formatToolbar";
import type { ImageAlign, ImageAttrs } from "../editor/image";
import { imageActions, revealLabel } from "./actions";
import { useImageUi } from "./store";
import { classify } from "./useImageSrc";

const EXIT_MS = 120;

const ALIGN_BUTTONS: {
  align: ImageAlign;
  label: string;
  icon: typeof AlignHorizontalJustifyStart;
}[] = [
  { align: "left", label: "Align left", icon: AlignHorizontalJustifyStart },
  { align: "center", label: "Centre", icon: AlignHorizontalJustifyCenter },
  { align: "right", label: "Align right", icon: AlignHorizontalJustifyEnd },
];

export function ImageToolbar({ editor }: { editor: Editor | null }) {
  const bar = useRef<HTMLDivElement>(null);
  const [focusInBar, setFocusInBar] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [side, setSide] = useState<"above" | "below">("below");
  const editingAlt = useImageUi((s) => s.editingAlt);
  const setEditingAlt = useImageUi((s) => s.setEditingAlt);
  const actions = useMemo(() => (editor ? imageActions(editor) : null), [editor]);

  const state = useEditorState({
    editor,
    selector: ({ editor: e }) => {
      if (!e) return null;
      const { selection } = e.state;
      if (!(selection instanceof NodeSelection) || selection.node.type.name !== "image") {
        return {
          pos: null as number | null,
          focused: e.isFocused,
          attrs: null as ImageAttrs | null,
        };
      }
      return {
        pos: selection.from,
        focused: e.isFocused,
        attrs: selection.node.attrs as ImageAttrs,
      };
    },
  });

  const pos = state?.pos ?? null;
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset per image.
  useEffect(() => {
    setDismissed(false);
    if (pos === null) setEditingAlt(false);
  }, [pos]);

  // The image counts as selected only while the note or this bar holds focus; Esc hides the bar
  // but leaves the image selected.
  const active = pos !== null && (!!state?.focused || focusInBar || menuOpen || editingAlt);
  const setActiveImage = useImageUi((s) => s.setActiveImage);
  useEffect(() => {
    setActiveImage(active ? pos : null);
  }, [active, pos, setActiveImage]);
  useEffect(() => () => setActiveImage(null), [setActiveImage]);

  const visible = !!editor?.isEditable && active && !dismissed;

  const [mounted, setMounted] = useState(false);
  useEffect(() => {
    if (visible) {
      setMounted(true);
      return;
    }
    const t = setTimeout(() => setMounted(false), EXIT_MS);
    return () => clearTimeout(t);
  }, [visible]);

  const place = useCallback(() => {
    const el = bar.current;
    if (!editor || !el || editor.isDestroyed || pos === null) return;
    const dom = editor.view.nodeDOM(pos);
    // Centred on the picture, under its caption.
    const figure = dom instanceof HTMLElement ? dom.querySelector(".nt-image") : null;
    const frame = figure?.querySelector(".nt-image-frame");
    const rect =
      figure && frame
        ? (() => {
            const f = frame.getBoundingClientRect();
            const g = figure.getBoundingClientRect();
            return { left: f.left, right: f.right, top: f.top, bottom: g.bottom };
          })()
        : posToDOMRect(editor.view, pos, pos + 1);
    const p = placeToolbar(
      rect,
      { width: el.offsetWidth, height: el.offsetHeight },
      { width: window.innerWidth, height: window.innerHeight },
    );
    el.style.left = `${Math.round(p.left)}px`;
    el.style.top = `${Math.round(p.top)}px`;
    setSide(p.side);
  }, [editor, pos]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: size, placement and the alt field move it.
  useLayoutEffect(() => {
    if (visible) place();
  }, [visible, state, editingAlt, place]);

  useEffect(() => {
    if (!visible) return;
    // The image's size changes as it loads or is resized; follow it.
    const observer = new ResizeObserver(() => place());
    const dom = editor && pos !== null ? editor.view.nodeDOM(pos) : null;
    if (dom instanceof HTMLElement) observer.observe(dom);
    window.addEventListener("scroll", place, true);
    window.addEventListener("resize", place);
    return () => {
      observer.disconnect();
      window.removeEventListener("scroll", place, true);
      window.removeEventListener("resize", place);
    };
  }, [visible, place, editor, pos]);

  const dismiss = () => {
    setDismissed(true);
    setEditingAlt(false);
    setFocusInBar(false);
    editor?.commands.focus();
  };
  useEscapeLayer(visible && !menuOpen, dismiss);

  if (!editor || !actions || !state || pos === null || !state.attrs || !(visible || mounted))
    return null;

  const attrs = state.attrs;
  const source = classify(attrs.src);
  const shown = source.kind === "ready" || source.kind === "resolving";
  const isFile = isAttachmentPath(attrs.src);
  const tip = side === "above" ? "top" : "bottom";

  return createPortal(
    <FormatBar
      ref={bar}
      label="Image"
      side={side}
      closing={!visible}
      onFocusEnter={() => setFocusInBar(true)}
      onFocusLeave={() => {
        setFocusInBar(false);
        if (!menuOpen) setEditingAlt(false);
      }}
      style={{ left: -9999, top: -9999 }}
    >
      {editingAlt ? (
        <FormatBarField
          icon={TextCursorInput}
          label="Alt text"
          placeholder="Describe the image for screen readers"
          defaultValue={attrs.alt ?? ""}
          onSubmit={(value) => {
            actions.setAlt(pos, value);
            setEditingAlt(false);
            editor.commands.focus();
          }}
          onCancel={() => {
            setEditingAlt(false);
            editor.commands.focus();
          }}
        >
          <IconButton icon={Check} label="Save alt text" type="submit" noTooltip />
        </FormatBarField>
      ) : (
        <>
          {ALIGN_BUTTONS.map((b) => (
            <IconButton
              key={b.align}
              icon={b.icon}
              label={b.label}
              tooltipSide={tip}
              pressed={(attrs.align ?? "left") === b.align}
              onClick={() => actions.setAlign(pos, b.align)}
            />
          ))}
          <FormatBarDivider />
          <IconButton
            icon={Captions}
            label={attrs.title ? "Edit caption" : "Add caption"}
            tooltipSide={tip}
            pressed={!!attrs.title}
            disabled={!shown}
            onClick={() => actions.editCaption(pos)}
          />
          <IconButton
            icon={TextCursorInput}
            label="Alt text"
            tooltipSide={tip}
            pressed={!!attrs.alt}
            onClick={() => setEditingAlt(true)}
          />
          <IconButton
            icon={Maximize2}
            label="View image"
            tooltipSide={tip}
            disabled={!shown}
            onClick={() => void actions.view(pos)}
          />
          <FormatBarDivider />
          <Menu onOpenChange={setMenuOpen}>
            <MenuTrigger asChild>
              <IconButton icon={Ellipsis} label="More" tooltipSide={tip} />
            </MenuTrigger>
            <MenuContent align="end">
              <MenuItem icon={Copy} disabled={!shown} onSelect={() => void actions.copy(pos)}>
                Copy image
              </MenuItem>
              <MenuItem
                icon={Download}
                disabled={!isFile}
                onSelect={() => void actions.saveAs(pos)}
              >
                Save image as…
              </MenuItem>
              {isTauri && (
                <MenuItem
                  icon={FolderOpen}
                  disabled={!isFile}
                  onSelect={() => void actions.reveal(pos)}
                >
                  {revealLabel}
                </MenuItem>
              )}
              <MenuItem icon={Replace} onSelect={() => void actions.replace(pos)}>
                Replace…
              </MenuItem>
              <MenuSeparator />
              <MenuItem icon={Trash} danger onSelect={() => actions.remove(pos)}>
                Delete image
              </MenuItem>
            </MenuContent>
          </Menu>
        </>
      )}
    </FormatBar>,
    document.body,
  );
}
