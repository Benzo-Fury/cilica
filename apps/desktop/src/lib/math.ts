/**
 * Drawing LaTeX with KaTeX, shared by the note editor and the equation editor. KaTeX ships with
 * the app, so equations draw offline and nothing leaves the machine.
 */
import katex from "katex";

export type MathRender = { ok: true; html: string } | { ok: false; error: string };

/** KaTeX's HTML for `latex`, or why it cannot be drawn, in words for people. */
export function renderMath(latex: string, display: boolean): MathRender {
  try {
    const html = katex.renderToString(latex, {
      displayMode: display,
      throwOnError: true,
      strict: "ignore",
      output: "htmlAndMathml",
    });
    return { ok: true, html };
  } catch (error) {
    const raw = error instanceof Error ? error.message : String(error);
    const reason = raw
      .replace(/^KaTeX parse error:\s*/, "")
      .replace(/\s*at (?:position|end of input)[\s\S]*$/, "")
      .trim();
    return { ok: false, error: reason || "This LaTeX can’t be read" };
  }
}

/** A formula people reach for often, offered under Formulas in the equation editor. */
export interface CommonFormula {
  label: string;
  latex: string;
}

export const COMMON_FORMULAS: readonly CommonFormula[] = [
  { label: "Quadratic formula", latex: "x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}" },
  { label: "Pythagoras", latex: "a^2 + b^2 = c^2" },
  { label: "Area of a circle", latex: "A = \\pi r^2" },
  { label: "Compound interest", latex: "A = P\\left(1 + \\frac{r}{n}\\right)^{nt}" },
  { label: "Mean", latex: "\\bar{x} = \\frac{1}{n}\\sum_{i=1}^{n} x_i" },
  {
    label: "Standard deviation",
    latex: "\\sigma = \\sqrt{\\frac{1}{n}\\sum_{i=1}^{n} (x_i - \\mu)^2}",
  },
  { label: "Speed", latex: "v = \\frac{d}{t}" },
  { label: "Mass–energy", latex: "E = mc^2" },
];

/**
 * A building block in the equation editor's palette. `visual` is what the visual editor
 * inserts (`#@` takes the selection, `#?` is an empty box to fill); `latex` is what the LaTeX
 * tab inserts; `icon` is drawn on the button.
 */
export interface MathTemplate {
  name: string;
  icon: string;
  visual: string;
  latex: string;
}

export const MATH_STRUCTURES: readonly MathTemplate[] = [
  { name: "Fraction", icon: "\\frac{a}{b}", visual: "\\frac{#@}{#?}", latex: "\\frac{a}{b}" },
  { name: "Square root", icon: "\\sqrt{x}", visual: "\\sqrt{#@}", latex: "\\sqrt{x}" },
  { name: "Root", icon: "\\sqrt[n]{x}", visual: "\\sqrt[#?]{#@}", latex: "\\sqrt[n]{x}" },
  { name: "Power", icon: "x^{n}", visual: "#@^{#?}", latex: "^{n}" },
  { name: "Subscript", icon: "x_{i}", visual: "#@_{#?}", latex: "_{i}" },
  { name: "Brackets", icon: "(x)", visual: "\\left(#@\\right)", latex: "\\left(x\\right)" },
  {
    name: "Sum",
    icon: "\\sum",
    visual: "\\sum_{#?}^{#?}#@",
    latex: "\\sum_{i=1}^{n}",
  },
  { name: "Integral", icon: "\\int", visual: "\\int_{#?}^{#?}#@", latex: "\\int_{a}^{b}" },
  { name: "Limit", icon: "\\lim", visual: "\\lim_{#?}#@", latex: "\\lim_{x \\to 0}" },
  {
    name: "Matrix",
    icon: "\\left(\\begin{smallmatrix} a & b \\\\ c & d \\end{smallmatrix}\\right)",
    visual: "\\begin{pmatrix} #? & #? \\\\ #? & #? \\end{pmatrix}",
    latex: "\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}",
  },
];

const symbol = (name: string, latex: string): MathTemplate => ({
  name,
  icon: latex,
  visual: latex,
  latex,
});

export const MATH_SYMBOLS: readonly MathTemplate[] = [
  symbol("Alpha", "\\alpha"),
  symbol("Beta", "\\beta"),
  symbol("Theta", "\\theta"),
  symbol("Pi", "\\pi"),
  symbol("Sigma", "\\sigma"),
  symbol("Delta", "\\Delta"),
  symbol("Plus or minus", "\\pm"),
  symbol("Times", "\\times"),
  symbol("Divided by", "\\div"),
  symbol("Less than or equal to", "\\le"),
  symbol("Greater than or equal to", "\\ge"),
  symbol("Not equal to", "\\ne"),
  symbol("Approximately", "\\approx"),
  symbol("Infinity", "\\infty"),
  symbol("Arrow", "\\to"),
];
