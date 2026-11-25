/**
 * The visual equation field: MathLive's `<math-field>`, where `/` makes a fraction, `^` a power
 * and `sqrt` a root, and the arrow keys and Tab move between the boxes. It reads and writes
 * LaTeX, so what it saves is the same Markdown as typing `$…$` by hand.
 *
 * MathLive is large, so it loads the first time an equation is opened. It draws with the same
 * KaTeX fonts as the note (bundled through katex.css), and its sounds, menu and on-screen
 * keyboard are off.
 */
import type { MathfieldElement } from "mathlive";
import { type Ref, useEffect, useImperativeHandle, useRef, useState } from "react";
import { Spinner } from "./Spinner";

export interface MathFieldHandle {
  /** Insert a template (`#@` takes the selection, `#?` is a box to fill). */
  insert: (template: string) => void;
  focus: () => void;
}

let loading: Promise<typeof MathfieldElement> | null = null;

/** Loads MathLive once and configures it for Cilica. */
function loadMathLive(): Promise<typeof MathfieldElement> {
  loading ??= import("mathlive").then(({ MathfieldElement: Field }) => {
    // Fonts come from katex.css; no sounds or keypress clicks.
    Field.fontsDirectory = null;
    Field.soundsDirectory = null;
    Field.keypressSound = null;
    Field.plonkSound = null;
    return Field;
  });
  return loading;
}

export function MathField({
  value,
  onChange,
  onSubmit,
  label,
  ref,
}: {
  value: string;
  onChange: (latex: string) => void;
  /** Enter in the field, with the field's LaTeX at that moment. */
  onSubmit: (latex: string) => void;
  label: string;
  ref?: Ref<MathFieldHandle>;
}) {
  const host = useRef<HTMLDivElement>(null);
  const field = useRef<MathfieldElement | null>(null);
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);
  const latest = useRef({ onChange, onSubmit });
  latest.current = { onChange, onSubmit };
  const initial = useRef(value);

  useEffect(() => {
    let cancelled = false;
    loadMathLive()
      .then(() => {
        if (cancelled || !host.current) return;
        // Created through the registry: `new MathfieldElement()` throws when Vite hands this
        // module a second copy of the class.
        const mf = document.createElement("math-field") as MathfieldElement;
        host.current.replaceChildren(mf);
        mf.mathVirtualKeyboardPolicy = "manual";
        mf.smartFence = true;
        mf.menuItems = [];
        mf.setAttribute("aria-label", label);
        mf.className = "nt-math-field";
        mf.value = initial.current;
        mf.addEventListener("input", () => latest.current.onChange(mf.getValue("latex")));
        // Enter saves. (MathLive's own `change` also fires on blur, which would save when you
        // only switched tabs or opened Formulas.)
        mf.addEventListener("keydown", (event) => {
          if (event.key !== "Enter" || event.shiftKey || event.isComposing) return;
          event.preventDefault();
          event.stopPropagation();
          latest.current.onSubmit(mf.getValue("latex"));
        });
        field.current = mf;
        setReady(true);
        requestAnimationFrame(() => mf.focus());
      })
      .catch((error: unknown) => {
        console.error("The visual equation editor didn’t load", error);
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
      field.current?.remove();
      field.current = null;
    };
  }, [label]);

  // Follow changes made elsewhere (the LaTeX tab, a formula picked from the menu).
  useEffect(() => {
    const mf = field.current;
    if (mf && mf.getValue("latex") !== value) mf.value = value;
  }, [value]);

  useImperativeHandle(ref, () => ({
    insert: (template) => {
      const mf = field.current;
      if (!mf) return;
      mf.insert(template, { selectionMode: "placeholder", focus: true });
      latest.current.onChange(mf.getValue("latex"));
    },
    focus: () => field.current?.focus(),
  }));

  return (
    <div className="nt-math-field-host" data-ready={ready || undefined}>
      <div ref={host} className="nt-math-field-slot" />
      {!ready && !failed && <Spinner label="Loading the visual editor" />}
      {failed && (
        <p className="nt-math-field-failed">
          The visual editor didn’t load. Use the LaTeX tab to edit this equation.
        </p>
      )}
    </div>
  );
}
