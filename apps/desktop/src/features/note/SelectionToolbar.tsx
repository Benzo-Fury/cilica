/**
 * The formatting toolbar that appears over selected text in a note: bold, italic,
 * strikethrough, inline code, headings, quote and link. Everything it offers round-trips
 * through Markdown. It waits until the mouse is released, hides on Esc, and follows the
 * selection as the page scrolls.
 */
import { type Editor, posToDOMRect } from "@tiptap/core";
import { useEditorState } from "@tiptap/react";
import {
  Bold,
  Check,
  Code,
  Heading1,
  Heading2,
  Italic,
  Link,
  type LucideIcon,
  Strikethrough,
  TextQuote,
  Unlink,
} from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { formatAccelerator } from "@/app/keybinds/registry";
import { useEscapeLayer } from "@/app/layers";
import { isMac } from "@/app/platform";
import { FormatBar, FormatBarDivider, FormatBarField, IconButton } from "@/ui";
import { hasFormattableSelection, normalizeHref, placeToolbar } from "./editor/formatToolbar";

/** How long the exit fade runs before the bar unmounts (`--dur-fast`). */
const EXIT_MS = 120;

interface Action {
  id: string;
  icon: LucideIcon;
  label: string;
  /** TipTap's default binding, shown in the tooltip. */
  keys?: string;
  active: (editor: Editor) => boolean;
  can: (editor: Editor) => boolean;
  run: (editor: Editor) => void;
}

const MARKS: Action[] = [
  {
    id: "bold",
    icon: Bold,
    label: "Bold",
    keys: "Mod+B",
    active: (e) => e.isActive("bold"),
    can: (e) => e.can().toggleBold(),
    run: (e) => e.chain().focus().toggleBold().run(),
  },
  {
    id: "italic",
    icon: Italic,
    label: "Italic",
    keys: "Mod+I",
    active: (e) => e.isActive("italic"),
    can: (e) => e.can().toggleItalic(),
    run: (e) => e.chain().focus().toggleItalic().run(),
  },
  {
    id: "strike",
    icon: Strikethrough,
    label: "Strikethrough",
    keys: "Mod+Shift+S",
    active: (e) => e.isActive("strike"),
    can: (e) => e.can().toggleStrike(),
    run: (e) => e.chain().focus().toggleStrike().run(),
  },
  {
    // ⌘E belongs to Notes ⇄ Transcript, so no shortcut is advertised here.
    id: "code",
    icon: Code,
    label: "Inline code",
    active: (e) => e.isActive("code"),
    can: (e) => e.can().toggleCode(),
    run: (e) => e.chain().focus().toggleCode().run(),
  },
];

const BLOCKS: Action[] = [
  {
    id: "h1",
    icon: Heading1,
    label: "Heading 1",
    keys: "Mod+Alt+1",
    active: (e) => e.isActive("heading", { level: 1 }),
    can: (e) => e.can().toggleHeading({ level: 1 }),
    run: (e) => e.chain().focus().toggleHeading({ level: 1 }).run(),
  },
  {
    id: "h2",
    icon: Heading2,
    label: "Heading 2",
    keys: "Mod+Alt+2",
    active: (e) => e.isActive("heading", { level: 2 }),
    can: (e) => e.can().toggleHeading({ level: 2 }),
    run: (e) => e.chain().focus().toggleHeading({ level: 2 }).run(),
  },
  {
    id: "quote",
    icon: TextQuote,
    label: "Quote",
    keys: "Mod+Shift+B",
    active: (e) => e.isActive("blockquote"),
    can: (e) => e.can().toggleBlockquote(),
    run: (e) => e.chain().focus().toggleBlockquote().run(),
  },
];

const ALL = [...MARKS, ...BLOCKS];

export function SelectionToolbar({ editor }: { editor: Editor | null }) {
  const bar = useRef<HTMLDivElement>(null);
  const [pointerDown, setPointerDown] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [editingLink, setEditingLink] = useState(false);
  const [side, setSide] = useState<"above" | "below">("above");

  const state = useEditorState({
    editor,
    selector: ({ editor: e }) => {
      if (!e) return null;
      const { from, to } = e.state.selection;
      const selectable = hasFormattableSelection(e.state, e.isEditable);
      // Typing with a caret should not pay for seven dry-run commands per keystroke.
      if (!selectable)
        return {
          from,
          to,
          selectable,
          focused: e.isFocused,
          active: {} as Record<string, boolean>,
          enabled: {} as Record<string, boolean>,
          link: false,
          href: "",
        };
      return {
        from,
        to,
        selectable,
        focused: e.isFocused,
        active: Object.fromEntries(ALL.map((a) => [a.id, a.active(e)])) as Record<string, boolean>,
        enabled: Object.fromEntries(ALL.map((a) => [a.id, a.can(e)])) as Record<string, boolean>,
        link: e.isActive("link"),
        href: (e.getAttributes("link").href as string | undefined) ?? "",
      };
    },
  });

  // Wait for the drag to finish before showing, so the bar does not chase the mouse.
  useEffect(() => {
    if (!editor) return;
    const dom = editor.view.dom;
    const down = (event: PointerEvent) => event.button === 0 && setPointerDown(true);
    const up = () => setPointerDown(false);
    dom.addEventListener("pointerdown", down);
    window.addEventListener("pointerup", up);
    window.addEventListener("blur", up);
    return () => {
      dom.removeEventListener("pointerdown", down);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("blur", up);
    };
  }, [editor]);

  // A new selection brings a dismissed bar back.
  const from = state?.from;
  const to = state?.to;
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset on every selection change.
  useEffect(() => {
    setDismissed(false);
    setEditingLink(false);
  }, [from, to]);

  const visible =
    !!state?.selectable && !pointerDown && !dismissed && (state.focused || editingLink);

  // Keep it mounted for the exit fade.
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
    if (!editor || !el || editor.isDestroyed) return;
    const { from: a, to: b } = editor.state.selection;
    const rect = posToDOMRect(editor.view, a, b);
    const p = placeToolbar(
      rect,
      { width: el.offsetWidth, height: el.offsetHeight },
      { width: window.innerWidth, height: window.innerHeight },
    );
    el.style.left = `${Math.round(p.left)}px`;
    el.style.top = `${Math.round(p.top)}px`;
    setSide(p.side);
  }, [editor]);

  // Place before paint when it appears and whenever the selection or its contents change.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `state` and `editingLink` resize it.
  useLayoutEffect(() => {
    if (visible) place();
  }, [visible, state, editingLink, place]);

  useEffect(() => {
    if (!visible) return;
    window.addEventListener("scroll", place, true);
    window.addEventListener("resize", place);
    return () => {
      window.removeEventListener("scroll", place, true);
      window.removeEventListener("resize", place);
    };
  }, [visible, place]);

  // Esc hides it until the selection changes. From inside the bar the layer stack hears it;
  // inside the editor ProseMirror swallows Esc, so listen there too.
  const dismiss = () => {
    setDismissed(true);
    setEditingLink(false);
    editor?.commands.focus();
  };
  useEscapeLayer(visible, dismiss);
  const dismissRef = useRef(dismiss);
  dismissRef.current = dismiss;
  useEffect(() => {
    if (!editor || !visible) return;
    const dom = editor.view.dom;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.isComposing) dismissRef.current();
    };
    dom.addEventListener("keydown", onKey);
    return () => dom.removeEventListener("keydown", onKey);
  }, [editor, visible]);

  if (!editor || !state || !(visible || mounted)) return null;

  const applyLink = (value: string) => {
    const href = normalizeHref(value);
    const chain = editor.chain().focus().extendMarkRange("link");
    (href ? chain.setLink({ href }) : chain.unsetLink()).run();
    setEditingLink(false);
  };

  const button = (action: Action) => (
    <IconButton
      key={action.id}
      icon={action.icon}
      label={action.label}
      shortcut={action.keys ? formatAccelerator(action.keys, isMac) : undefined}
      tooltipSide={side === "above" ? "top" : "bottom"}
      pressed={state.active[action.id]}
      disabled={!state.enabled[action.id]}
      onClick={() => action.run(editor)}
    />
  );

  return createPortal(
    <FormatBar
      ref={bar}
      label="Format text"
      side={side}
      closing={!visible}
      onFocusLeave={() => setEditingLink(false)}
      // Placed by `place()` before paint; start off-screen so the first frame never flashes.
      style={{ left: -9999, top: -9999 }}
    >
      {editingLink ? (
        <FormatBarField
          icon={Link}
          label="Link address"
          placeholder="Paste or type a link"
          defaultValue={state.href}
          onSubmit={applyLink}
          onCancel={() => {
            setEditingLink(false);
            editor.commands.focus();
          }}
        >
          <IconButton icon={Check} label="Apply link" type="submit" noTooltip />
          {state.link && (
            <IconButton icon={Unlink} label="Remove link" noTooltip onClick={() => applyLink("")} />
          )}
        </FormatBarField>
      ) : (
        <>
          {MARKS.map(button)}
          <FormatBarDivider />
          {BLOCKS.map(button)}
          <FormatBarDivider />
          <IconButton
            icon={Link}
            label={state.link ? "Edit link" : "Link"}
            tooltipSide={side === "above" ? "top" : "bottom"}
            pressed={state.link}
            disabled={state.active.code}
            onClick={() => setEditingLink(true)}
          />
        </>
      )}
    </FormatBar>,
    document.body,
  );
}
