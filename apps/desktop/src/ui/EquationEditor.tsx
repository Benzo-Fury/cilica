/**
 * The equation editor that opens over an equation in a note, on strong glass. Two ways to write
 * the same LaTeX:
 * - Visual: type naturally (`/` for a fraction, `^` for a power) or pick a building block;
 * - LaTeX: the source, with a live preview and the reason when it can't be drawn.
 * Formulas offers common equations to start from. Enter (Visual) or ⌘↩ saves, Esc cancels.
 */
import { ChevronDown, Sigma } from "lucide-react";
import { type CSSProperties, type KeyboardEvent, type Ref, useMemo, useRef, useState } from "react";
import { isMac } from "@/app/platform";
import {
  COMMON_FORMULAS,
  MATH_STRUCTURES,
  MATH_SYMBOLS,
  type MathTemplate,
  renderMath,
} from "@/lib/math";
import { Button } from "./Button";
import { cn } from "./cn";
import { Icon } from "./Icon";
import { Kbd } from "./Kbd";
import { MathField, type MathFieldHandle } from "./MathField";
import { Menu, MenuContent, MenuItem, MenuTrigger } from "./Menu";
import { SegmentedControl } from "./SegmentedControl";

export type EquationMode = "visual" | "latex";

const MODES = [
  { value: "visual", label: "Visual" },
  { value: "latex", label: "LaTeX" },
] as const;

/** One row of buttons at a time, so the panel stays small. */
const PALETTES = [
  { value: "blocks", label: "Building blocks", items: MATH_STRUCTURES },
  { value: "symbols", label: "Symbols", items: MATH_SYMBOLS },
] as const;
type PaletteName = (typeof PALETTES)[number]["value"];

/** The row people used last, for the next equation this session. */
let lastPalette: PaletteName = "blocks";

/** KaTeX HTML for a palette icon or menu preview (they are fixed, so always draw). */
function Tex({ latex, display = false }: { latex: string; display?: boolean }) {
  const html = useMemo(() => {
    const r = renderMath(latex, display);
    return r.ok ? r.html : latex;
  }, [latex, display]);
  // biome-ignore lint/security/noDangerouslySetInnerHtml: KaTeX output for fixed, app-defined LaTeX.
  return <span className="nt-eq-tex" dangerouslySetInnerHTML={{ __html: html }} />;
}

export interface EquationEditorProps {
  /** On its own line (block) or inside a sentence. */
  display: boolean;
  value: string;
  onChange: (latex: string) => void;
  mode: EquationMode;
  onModeChange: (mode: EquationMode) => void;
  /** Save. Enter in the visual field passes its LaTeX, which may be newer than `value`. */
  onDone: (latest?: string) => void;
  onCancel: () => void;
  onRemove: () => void;
  /** Where it sits (fixed, under or over the equation). */
  style?: CSSProperties;
  side?: "above" | "below";
  ref?: Ref<HTMLDivElement>;
}

export function EquationEditor({
  display,
  value,
  onChange,
  mode,
  onModeChange,
  onDone,
  onCancel,
  onRemove,
  style,
  side = "below",
  ref,
}: EquationEditorProps) {
  const field = useRef<MathFieldHandle>(null);
  const [paletteName, setPaletteName] = useState<PaletteName>(lastPalette);
  const palette = PALETTES.find((p) => p.value === paletteName) ?? PALETTES[0];
  const source = useRef<HTMLTextAreaElement>(null);
  const preview = useMemo(() => (value.trim() ? renderMath(value, true) : null), [value]);
  const kind = display ? "Equation block" : "Inline equation";

  const insert = (template: MathTemplate) => {
    if (mode === "visual") {
      field.current?.insert(template.visual);
      return;
    }
    const ta = source.current;
    if (!ta) return;
    const { selectionStart: a, selectionEnd: b } = ta;
    const next = value.slice(0, a) + template.latex + value.slice(b);
    onChange(next);
    requestAnimationFrame(() => {
      ta.focus();
      ta.selectionStart = ta.selectionEnd = a + template.latex.length;
    });
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
    } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      event.stopPropagation();
      onDone();
    }
  };

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label={`Edit ${kind.toLowerCase()}`}
      className="nt-eq nt-glass-strong"
      data-side={side}
      style={style}
      onKeyDown={onKeyDown}
    >
      <div className="nt-eq-head">
        <span className="nt-eq-kind">{kind}</span>
        <div className="nt-eq-head-actions">
          {/* Not modal: a modal menu blocks the page, so a click back on this panel would land
              outside it and close it. */}
          <Menu modal={false}>
            <MenuTrigger asChild>
              <button type="button" className="nt-eq-formulas">
                <Icon icon={Sigma} size="sm" />
                Formulas
                <Icon icon={ChevronDown} size="sm" />
              </button>
            </MenuTrigger>
            <MenuContent
              align="end"
              className="nt-eq-formula-menu"
              // Back to the equation, not the Formulas button, unless a click put focus elsewhere.
              onCloseAutoFocus={(event) => {
                event.preventDefault();
                requestAnimationFrame(() => {
                  if (document.activeElement !== document.body) return;
                  if (mode === "visual") field.current?.focus();
                  else source.current?.focus();
                });
              }}
            >
              {COMMON_FORMULAS.map((f) => (
                <MenuItem key={f.label} onSelect={() => onChange(f.latex)}>
                  <span className="nt-eq-formula">
                    <span className="nt-eq-formula-label">{f.label}</span>
                    <Tex latex={f.latex} />
                  </span>
                </MenuItem>
              ))}
            </MenuContent>
          </Menu>
          <SegmentedControl label="Edit as" options={MODES} value={mode} onChange={onModeChange} />
        </div>
      </div>

      {mode === "visual" ? (
        <div className="nt-eq-well">
          <MathField ref={field} value={value} onChange={onChange} onSubmit={onDone} label={kind} />
        </div>
      ) : (
        <>
          <textarea
            ref={source}
            className="nt-textarea nt-eq-source"
            value={value}
            spellCheck={false}
            aria-label="LaTeX"
            placeholder="\frac{a}{b}"
            // biome-ignore lint/a11y/noAutofocus: the dialog opened to edit this.
            autoFocus
            onChange={(e) => onChange(e.target.value)}
          />
          <div
            className={cn("nt-eq-preview", preview && !preview.ok && "is-error")}
            aria-live="polite"
          >
            {!preview ? (
              <span className="nt-eq-hint">Type LaTeX to see it here</span>
            ) : preview.ok ? (
              // biome-ignore lint/security/noDangerouslySetInnerHtml: KaTeX output, which escapes its input.
              <span className="nt-eq-tex" dangerouslySetInnerHTML={{ __html: preview.html }} />
            ) : (
              <span>Can’t show this yet: {preview.error}.</span>
            )}
          </div>
        </>
      )}

      <div className="nt-eq-palettes">
        <div className="nt-eq-tabs" role="tablist" aria-label="Insert">
          {PALETTES.map((p) => (
            <button
              key={p.value}
              type="button"
              role="tab"
              className="nt-eq-tab"
              aria-selected={p.value === palette.value}
              // Keep the caret in the field while switching.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => {
                lastPalette = p.value;
                setPaletteName(p.value);
              }}
            >
              {p.label}
            </button>
          ))}
        </div>
        <div
          className="nt-eq-palette"
          data-palette={palette.value}
          role="toolbar"
          aria-label={palette.label}
        >
          {palette.items.map((t) => (
            <button
              key={t.name}
              type="button"
              className="nt-eq-chip"
              aria-label={t.name}
              title={t.name}
              // Keep the caret in the field while picking.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => insert(t)}
            >
              <Tex latex={t.icon} />
            </button>
          ))}
        </div>
      </div>

      <div className="nt-eq-foot">
        <span className="nt-eq-keys">
          <Kbd keys={["Esc"]} /> to cancel
          <Kbd keys={isMac ? ["⌘", "↩"] : ["Ctrl", "Enter"]} /> to save
        </span>
        <div className="nt-eq-buttons">
          <Button variant="ghost" size="sm" onClick={onRemove}>
            Remove
          </Button>
          <Button variant="primary" size="sm" onClick={() => onDone()}>
            Done
          </Button>
        </div>
      </div>
    </div>
  );
}
