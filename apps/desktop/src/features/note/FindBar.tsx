import type { Editor } from "@tiptap/core";
import { ChevronDown, ChevronUp, Search, X } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { formatAccelerator } from "@/app/keybinds";
import { useEscapeLayer } from "@/app/layers";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { isMac } from "@/app/platform";
import { Glass, Icon, IconButton } from "@/ui";
import { findKey } from "./editor/findInNote";

/** ⌘F: a floating glass find bar. Enter / ⇧Enter step through matches; Esc closes. */
export function FindBar({
  editor,
  focusTick,
  onClose,
}: {
  editor: Editor | null;
  focusTick: number;
  onClose: () => void;
}) {
  const reduced = useReducedMotion();
  const input = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState(() => {
    if (!editor) return "";
    const { from, to } = editor.state.selection;
    const selected = editor.state.doc.textBetween(from, to, " ");
    return selected.length < 80 ? selected : "";
  });
  const [counts, setCounts] = useState({ current: 0, total: 0 });

  const close = () => {
    editor?.commands.clearFind();
    onClose();
    editor?.commands.focus();
  };
  useEscapeLayer(true, close);

  // biome-ignore lint/correctness/useExhaustiveDependencies: refocus when ⌘F is pressed again
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, [focusTick]);

  useEffect(() => {
    if (!editor) return;
    editor.commands.setFindQuery(query);
    const update = () => {
      const state = findKey.getState(editor.state);
      setCounts({
        current: state?.matches.length ? state.current + 1 : 0,
        total: state?.matches.length ?? 0,
      });
    };
    update();
    editor.on("transaction", update);
    return () => {
      editor.off("transaction", update);
    };
  }, [editor, query]);

  useEffect(() => () => void editor?.commands.clearFind(), [editor]);

  const step = (direction: 1 | -1) =>
    direction === 1 ? editor?.commands.findNext() : editor?.commands.findPrevious();

  return (
    <motion.div
      className="nt-findbar-wrap"
      initial={{ opacity: 0, y: reduced ? 0 : -6, scale: reduced ? 1 : 0.98 }}
      animate={{
        opacity: 1,
        y: 0,
        scale: 1,
        transition: reduced ? reducedTransition : springs.snappy,
      }}
      exit={{ opacity: 0, y: reduced ? 0 : -4, transition: tweens.exit }}
    >
      <Glass strength="strong" className="nt-findbar" role="search">
        <Icon icon={Search} size="sm" className="nt-faint" />
        <input
          ref={input}
          value={query}
          placeholder="Find in note"
          aria-label="Find in note"
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              step(e.shiftKey ? -1 : 1);
            }
          }}
        />
        <span className="nt-findbar-count" aria-live="polite">
          {query.trim()
            ? counts.total
              ? `${counts.current} of ${counts.total}`
              : "No matches"
            : ""}
        </span>
        <IconButton
          icon={ChevronUp}
          size="sm"
          label="Previous match"
          shortcut={formatAccelerator("Shift+Enter", isMac)}
          disabled={!counts.total}
          onClick={() => step(-1)}
        />
        <IconButton
          icon={ChevronDown}
          size="sm"
          label="Next match"
          shortcut={formatAccelerator("Enter", isMac)}
          disabled={!counts.total}
          onClick={() => step(1)}
        />
        <IconButton
          icon={X}
          iconMotion="x"
          size="sm"
          label="Close find"
          shortcut={formatAccelerator("Escape", isMac)}
          onClick={close}
        />
      </Glass>
    </motion.div>
  );
}
