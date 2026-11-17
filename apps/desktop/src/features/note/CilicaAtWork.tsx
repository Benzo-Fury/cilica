/**
 * Cilica at work in the open note: while a turn reads or edits this note, a marker washes the
 * section it is on and springs to the next one as the work moves, like someone moving around your
 * screen. Reading sweeps the note section by section; an edit jumps to the text it changes and
 * brings it into view (unless you are typing). The marker leaves when the turn ends.
 */
import type { Node as PmNode } from "@tiptap/pm/model";
import type { Editor } from "@tiptap/react";
import { Eye, PencilLine } from "lucide-react";
import { useReducedMotion } from "motion/react";
import { type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { type Focus, useCilicaActivity } from "@/features/assistant/activity";
import { useProposalStatus } from "@/features/proposals/hooks";
import { CilicaMarker, type MarkerBox } from "@/ui";
import { rangesOf } from "./aiGlow";

/** How long the reading sweep stays on a section before moving on. */
const SWEEP_MS = 650;

type Span = [first: number, last: number];

/** Top-level blocks grouped into sections: each heading starts one. Empty paragraphs are skipped. */
export function sectionsOf(doc: PmNode): Span[] {
  const spans: Span[] = [];
  doc.forEach((node, _offset, index) => {
    if (node.type.name === "paragraph" && node.textContent.trim() === "") return;
    const current = spans[spans.length - 1];
    if (!current || node.type.name === "heading") spans.push([index, index]);
    else current[1] = index;
  });
  return spans;
}

/** The top-level blocks holding `phrases`, as one span; the last block when none is found. */
export function spanOf(doc: PmNode, phrases: readonly string[]): Span | null {
  if (doc.childCount === 0) return null;
  const ranges = rangesOf(doc, phrases);
  if (ranges.length === 0) return [doc.childCount - 1, doc.childCount - 1];
  const indexes = ranges.map((r) => doc.resolve(r.from).index(0));
  return [Math.min(...indexes), Math.max(...indexes)];
}

function blockDom(editor: Editor, index: number): HTMLElement | null {
  let pos = 0;
  for (let i = 0; i < index; i++) pos += editor.state.doc.child(i).nodeSize;
  const dom = editor.view.nodeDOM(pos);
  return dom instanceof HTMLElement ? dom : null;
}

const PAD_X = 10;
const PAD_Y = 6;

function boxOf(editor: Editor, host: HTMLElement, span: Span): MarkerBox | null {
  const first = blockDom(editor, span[0]);
  const last = blockDom(editor, span[1]);
  if (!first || !last) return null;
  const origin = host.getBoundingClientRect();
  const a = first.getBoundingClientRect();
  const b = last.getBoundingClientRect();
  const left = Math.min(a.left, b.left) - origin.left - PAD_X;
  const right = Math.max(a.right, b.right) - origin.left + PAD_X;
  return {
    top: a.top - origin.top - PAD_Y,
    left,
    width: right - left,
    height: b.bottom - a.top + PAD_Y * 2,
  };
}

const LABELS: Record<Focus["mode"], string> = {
  reading: "Cilica is reading",
  editing: "Cilica is editing here",
  suggesting: "Cilica suggests a change here",
};

export function CilicaAtWork({
  editor,
  noteId,
  children,
}: {
  editor: Editor | null;
  noteId: string;
  children: ReactNode;
}) {
  const current = useCilicaActivity((s) => (s.focus?.noteId === noteId ? s.focus : null));
  // A suggested edit is marked until you answer it; then the marker leaves.
  const status = useProposalStatus(current?.proposalId);
  const focus = current?.proposalId && status && status !== "pending" ? null : current;
  const reduced = useReducedMotion() ?? false;
  const host = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState<MarkerBox | null>(null);
  const [step, setStep] = useState(0);
  const [layoutTick, setLayoutTick] = useState(0);

  // The reading sweep: move to the next section on a short beat (in place under Reduce motion).
  useEffect(() => {
    setStep(0);
    if (focus?.mode !== "reading" || reduced) return;
    const t = setInterval(() => setStep((s) => s + 1), SWEEP_MS);
    return () => clearInterval(t);
  }, [focus, reduced]);

  // Re-measure when the note's text or the column's width changes.
  useEffect(() => {
    if (!editor || !focus) return;
    const bump = () => setLayoutTick((t) => t + 1);
    editor.on("update", bump);
    const observer = new ResizeObserver(bump);
    observer.observe(editor.view.dom);
    return () => {
      editor.off("update", bump);
      observer.disconnect();
    };
  }, [editor, focus]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: layoutTick asks for a fresh measure
  useLayoutEffect(() => {
    if (!editor || editor.isDestroyed || !focus || !host.current) {
      setBox(null);
      return;
    }
    const doc = editor.state.doc;
    let span: Span | null;
    if (focus.mode === "reading" && focus.phrases.length === 0) {
      const sections = sectionsOf(doc);
      span = sections.length ? (sections[step % sections.length] ?? null) : null;
    } else {
      span = spanOf(doc, focus.phrases);
    }
    const next = span ? boxOf(editor, host.current, span) : null;
    setBox(next);
    // An edit you are not typing next to comes into view.
    if (next && span && focus.mode !== "reading" && !editor.isFocused) {
      blockDom(editor, span[0])?.scrollIntoView({
        block: "nearest",
        behavior: reduced ? "auto" : "smooth",
      });
    }
  }, [editor, focus, step, layoutTick, reduced]);

  return (
    <div className="nt-editor-host" ref={host}>
      <CilicaMarker
        box={box}
        label={focus ? LABELS[focus.mode] : ""}
        icon={focus?.mode === "reading" ? Eye : PencilLine}
      />
      {children}
    </div>
  );
}
