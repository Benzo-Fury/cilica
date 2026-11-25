import { Editor } from "@tiptap/core";
import { renderMath } from "@/lib/math";
import { noteExtensions } from "./extensions";
import { normalizeMarkdown } from "./markdown";
import { looksLikeMarkdown } from "./markdownInput";
import { escapeDollars, inlineMathStart, matchBlockMath, matchInlineMath } from "./math";

function load(markdown: string) {
  return new Editor({ extensions: noteExtensions(), content: markdown, contentType: "markdown" });
}

function roundTrip(markdown: string): string {
  const editor = load(markdown);
  const out = editor.getMarkdown();
  editor.destroy();
  return normalizeMarkdown(out);
}

function nodeTypes(markdown: string): string[] {
  const editor = load(markdown);
  const types: string[] = [];
  editor.state.doc.descendants((node) => {
    types.push(node.type.name);
  });
  editor.destroy();
  return types;
}

describe("inline equation syntax", () => {
  it.each([
    ["$E = mc^2$ rest", "E = mc^2"],
    ["$x$", "x"],
    ["$a\\$b$ after", "a\\$b"],
    ["$\\frac{1}{2}$.", "\\frac{1}{2}"],
  ])("reads %j", (src, latex) => {
    expect(matchInlineMath(src)?.latex).toBe(latex);
  });

  it.each([
    "$ x$", // space after the opening $
    "$x $", // space before the closing $
    "$5 and $10", // prices
    "$x$5", // closing $ right before a digit
    "$$x$$", // display syntax is not inline
  ])("leaves %j as text", (src) => {
    expect(matchInlineMath(src)).toBeNull();
  });

  it("skips escaped and doubled dollar signs when looking for a start", () => {
    expect(inlineMathStart("costs \\$5 and $x$")).toBe(14);
    expect(inlineMathStart("It costs $5 and $10.")).toBe(-1);
  });

  it("escapes only dollar signs that would read back as an equation", () => {
    expect(escapeDollars("It costs $5 and $10.")).toBe("It costs $5 and $10.");
    expect(escapeDollars("the $x$ variable")).toBe("the \\$x$ variable");
  });
});

describe("block equation syntax", () => {
  it("reads $$ lines around LaTeX", () => {
    expect(matchBlockMath("$$\na^2 + b^2\n$$\n\nNext")?.latex).toBe("a^2 + b^2");
    expect(matchBlockMath("$$\n\\begin{pmatrix} a \\\\ b \\end{pmatrix}\n$$")?.latex).toBe(
      "\\begin{pmatrix} a \\\\ b \\end{pmatrix}",
    );
  });

  it("reads $$…$$ on one line", () => {
    expect(matchBlockMath("$$x + 1$$\n")?.latex).toBe("x + 1");
  });

  it("needs a closing $$", () => {
    expect(matchBlockMath("$$\nnever closed")).toBeNull();
  });
});

describe("equations in the editor", () => {
  it.each([
    "Energy is $E = mc^2$ and that is all.",
    "$$\n\\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}\n$$",
    "Before\n\n$$\nA = P\\left(1 + \\frac{r}{n}\\right)^{nt}\n$$\n\nAfter",
    "- Kinetic: $E_k = \\tfrac{1}{2}mv^2$\n- Potential: $E_p = mgh$",
    "$$\n\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}\n$$",
    // Tables are padded to their widest cell, as elsewhere.
    "| Name | Formula   |\n| ---- | --------- |\n| Area | $\\pi r^2$ |",
    "Subscripts stay LaTeX: $x_1 + x_2$ and $a^{n+1}$.",
  ])("keeps %j exactly", (markdown) => {
    expect(roundTrip(markdown)).toBe(markdown);
  });

  it("keeps prices as text", () => {
    const markdown = "The studio is $1,850 a month and bills are $420.";
    expect(roundTrip(markdown)).toBe(markdown);
    expect(nodeTypes(markdown)).not.toContain("mathInline");
  });

  it("keeps an escaped dollar sign plain", () => {
    expect(nodeTypes("Write \\$x$ to show a dollar")).not.toContain("mathInline");
    expect(roundTrip("Write \\$x$ to show a dollar")).toBe("Write \\$x$ to show a dollar");
  });

  it("reads one-line blocks and writes them on their own lines", () => {
    expect(nodeTypes("$$x^2$$")).toContain("mathBlock");
    expect(roundTrip("$$x^2$$")).toBe("$$\nx^2\n$$");
  });

  it("starts a block where a paragraph is interrupted by $$", () => {
    expect(nodeTypes("Some text\n$$\ny = mx + b\n$$")).toEqual(["paragraph", "text", "mathBlock"]);
  });

  it("leaves code alone", () => {
    expect(roundTrip("`$x$` stays code")).toBe("`$x$` stays code");
    expect(nodeTypes("```\n$$\nx\n$$\n```")).not.toContain("mathBlock");
  });

  it("updates and removes an equation by position", () => {
    const editor = load("Area $r$ here");
    let pos = -1;
    editor.state.doc.descendants((node, at) => {
      if (node.type.name === "mathInline") pos = at;
    });
    editor.commands.setMathLatex(pos, "\\pi r^2");
    expect(normalizeMarkdown(editor.getMarkdown())).toBe("Area $\\pi r^2$ here");
    editor.commands.removeMath(pos);
    expect(normalizeMarkdown(editor.getMarkdown())).toBe("Area  here");
    editor.destroy();
  });

  it("turns $$ then Enter on its own line into a block", () => {
    const editor = load("Intro\n\n\\$\\$");
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
    const enter = new KeyboardEvent("keydown", { key: "Enter", bubbles: true });
    editor.view.someProp("handleKeyDown", (handle) => handle(editor.view, enter));
    expect(nodeTypes(editor.getMarkdown())).toContain("mathBlock");
    expect(editor.state.doc.lastChild?.type.name).toBe("mathBlock");
    editor.destroy();
  });

  it("inserts a block in place of an empty paragraph", () => {
    const editor = load("First\n\n");
    editor.commands.setContent("<p>First</p><p></p>");
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
    editor.commands.insertMath({ display: true, latex: "x" });
    expect(normalizeMarkdown(editor.getMarkdown())).toBe("First\n\n$$\nx\n$$");
    editor.destroy();
  });
});

describe("drawing", () => {
  it("draws good LaTeX", () => {
    const result = renderMath("\\frac{a}{b}", true);
    expect(result.ok).toBe(true);
  });

  it("says why bad LaTeX can't be drawn", () => {
    const result = renderMath("\\frac{s}{\\sqrt{n}", true);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).not.toMatch(/KaTeX parse error/);
  });
});

describe("pasting", () => {
  it("treats equations as Markdown but not prices", () => {
    expect(looksLikeMarkdown("The area is $\\pi r^2$.")).toBe(true);
    expect(looksLikeMarkdown("$$\nx\n$$")).toBe(true);
    expect(looksLikeMarkdown("It costs $5 and $10.")).toBe(false);
  });
});
