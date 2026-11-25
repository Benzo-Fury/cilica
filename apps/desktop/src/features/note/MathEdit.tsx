/**
 * Opens the equation editor over an equation in the note (clicked, Enter on a selected one, or
 * just inserted) and writes the LaTeX back. The LaTeX is only written when it changed, so an
 * equation opened and closed keeps its source exactly as it was typed or imported.
 *
 * Done, ⌘↩, Enter in the visual field or a click outside saves; Esc cancels (and removes an
 * equation that was just inserted empty); Remove deletes it. From the keyboard the equation stays
 * selected so typing carries on; a click outside lets go, so focus goes where you clicked. The editor follows the equation
 * if the note changes underneath it, and closes if the equation goes away.
 */
import type { Editor } from "@tiptap/core";
import {
  type RefObject,
  useCallback,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { renderMath } from "@/lib/math";
import { EquationEditor, type EquationMode } from "@/ui";
import { placeToolbar } from "./editor/formatToolbar";
import type { MathEditRequest, MathHandlers } from "./editor/math";

/** The tab people used last, for the next equation this session. */
let lastMode: EquationMode = "visual";

export function MathEdit({
  editor,
  handlers,
}: {
  editor: Editor | null;
  handlers: RefObject<MathHandlers | null>;
}) {
  const [open, setOpen] = useState<MathEditRequest | null>(null);
  const [value, setValue] = useState("");
  const [mode, setMode] = useState<EquationMode>(lastMode);
  const [place, setPlace] = useState<{ left: number; top: number; side: "above" | "below" } | null>(
    null,
  );
  const panel = useRef<HTMLDivElement>(null);
  const pos = useRef(0);

  const finish = useCallback(
    (action: "save" | "cancel" | "remove", latex: string, { letGo = false } = {}) => {
      if (!editor || !open) return;
      open.dom.classList.remove("is-editing");
      const at = pos.current;
      const chain = letGo ? editor.chain() : editor.chain().focus();
      const empty = !latex.trim();
      if (
        action === "remove" ||
        (empty && action === "save") ||
        (action === "cancel" && open.isNew && !open.latex.trim())
      ) {
        chain.removeMath(at).run();
      } else {
        if (action === "save" && latex !== open.latex) chain.setMathLatex(at, latex.trim());
        if (!letGo) chain.setNodeSelection(at);
        chain.run();
      }
      setOpen(null);
    },
    [editor, open],
  );

  useImperativeHandle(
    handlers,
    (): MathHandlers => ({
      onEdit: (request) => {
        open?.dom.classList.remove("is-editing");
        request.dom.classList.add("is-editing");
        pos.current = request.pos;
        setValue(request.latex);
        // LaTeX that can't be drawn opens on its source, so the reason shows.
        const broken = request.latex.trim() && !renderMath(request.latex, request.display).ok;
        setMode(broken ? "latex" : lastMode);
        setPlace(null);
        setOpen(request);
      },
    }),
  );

  // Follow the equation through edits made while the editor is open.
  useEffect(() => {
    if (!editor || !open) return;
    const onTransaction = ({
      transaction,
    }: {
      transaction: { docChanged: boolean; mapping: { map: (n: number) => number } };
    }) => {
      if (!transaction.docChanged) return;
      pos.current = transaction.mapping.map(pos.current);
      const node = editor.state.doc.nodeAt(pos.current);
      if (node?.type.name !== "mathInline" && node?.type.name !== "mathBlock") setOpen(null);
    };
    editor.on("transaction", onTransaction);
    return () => {
      editor.off("transaction", onTransaction);
    };
  }, [editor, open]);

  // Sit under the equation (over it when there is no room), and stay with it on scroll.
  useLayoutEffect(() => {
    if (!open) return;
    const update = () => {
      const box = open.dom.getBoundingClientRect();
      const size = panel.current?.getBoundingClientRect();
      setPlace(
        placeToolbar(
          box,
          { width: size?.width ?? 460, height: size?.height ?? 320 },
          { width: window.innerWidth, height: window.innerHeight },
        ),
      );
    };
    update();
    const observer = new ResizeObserver(update);
    if (panel.current) observer.observe(panel.current);
    window.addEventListener("scroll", update, true);
    window.addEventListener("resize", update);
    return () => {
      observer.disconnect();
      window.removeEventListener("scroll", update, true);
      window.removeEventListener("resize", update);
    };
  }, [open]);

  // A click outside saves, as in the rest of the note. Menus opened from the editor are inside.
  useEffect(() => {
    if (!open) return;
    const onDown = (event: MouseEvent) => {
      const target = event.target as Element | null;
      if (!target || panel.current?.contains(target)) return;
      if (target.closest("[data-radix-popper-content-wrapper]")) return;
      // MathLive's suggestion and matrix popovers live on <body>.
      if (target.closest('[id^="mathlive-"]')) return;
      finish("save", value, { letGo: true });
    };
    document.addEventListener("mousedown", onDown, true);
    return () => document.removeEventListener("mousedown", onDown, true);
  }, [open, value, finish]);

  if (!open) return null;
  return createPortal(
    <EquationEditor
      ref={panel}
      display={open.display}
      value={value}
      onChange={setValue}
      mode={mode}
      onModeChange={(next) => {
        lastMode = next;
        setMode(next);
      }}
      onDone={(latest) => finish("save", latest ?? value)}
      onCancel={() => finish("cancel", value)}
      onRemove={() => finish("remove", value)}
      side={place?.side}
      style={
        place
          ? { left: place.left, top: place.top }
          : // Measured on the first frame before it is placed.
            { left: 0, top: 0, visibility: "hidden" }
      }
    />,
    document.body,
  );
}
