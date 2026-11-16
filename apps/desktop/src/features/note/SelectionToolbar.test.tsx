import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Editor } from "@tiptap/core";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { useLinkResolver } from "@/features/links/resolver";
import type { NoteSummary } from "@/ipc/types";
import { TooltipProvider } from "@/ui";
import { noteExtensions } from "./editor/extensions";
import { SelectionToolbar } from "./SelectionToolbar";

// jsdom has no layout: placing the bar measures the selection, so give ranges an empty box.
beforeAll(() => {
  const empty = () => new DOMRect(0, 0, 0, 0);
  Range.prototype.getBoundingClientRect = empty;
  Range.prototype.getClientRects = () =>
    ({
      length: 0,
      item: () => null,
      [Symbol.iterator]: [][Symbol.iterator],
    }) as unknown as DOMRectList;
  Element.prototype.getClientRects = () =>
    ({
      length: 0,
      item: () => null,
      [Symbol.iterator]: [][Symbol.iterator],
    }) as unknown as DOMRectList;
});

let editor: Editor | null = null;
afterEach(() => {
  editor?.destroy();
  editor = null;
});

function setup(content = "Thsi is wrnog") {
  const element = document.createElement("div");
  document.body.append(element);
  editor = new Editor({
    element,
    extensions: noteExtensions(),
    content,
    contentType: "markdown",
  });
  const e = editor;
  // jsdom never gives a contenteditable focus, and the bar only shows in a focused editor.
  vi.spyOn(e.view, "hasFocus").mockReturnValue(true);
  render(
    <TooltipProvider>
      <SelectionToolbar editor={e} />
    </TooltipProvider>,
  );
  act(() => void fireEvent.focus(e.view.dom));
  const selectFirstWord = () =>
    act(() => void e.chain().focus().setTextSelection({ from: 1, to: 5 }).run());
  const select = (from: number, to: number) =>
    act(() => void e.chain().focus().setTextSelection({ from, to }).run());
  return { e, dom: e.view.dom, selectFirstWord, select };
}

const summary = (id: string, title: string, aliases: string[] = []): NoteSummary => ({
  id,
  title,
  aliases,
  snippet: "",
  category_id: null,
  kind: "manual",
  meeting_app: null,
  position: 0,
  created_at: "2026-11-01T00:00:00Z",
  updated_at: "2026-11-01T00:00:00Z",
  deleted_at: null,
});

const toolbar = () => screen.queryByRole("toolbar");

describe("SelectionToolbar", () => {
  it("shows under a selection", () => {
    const { selectFirstWord } = setup();
    selectFirstWord();
    expect(toolbar()).toBeInTheDocument();
  });

  it("applies superscript and subscript, one replacing the other", () => {
    const { e, select } = setup("H2O and x2");
    select(2, 3);
    fireEvent.click(screen.getByRole("button", { name: "Subscript" }));
    select(10, 11);
    fireEvent.click(screen.getByRole("button", { name: "Superscript" }));
    expect(e.getMarkdown().trim()).toBe("H~2~O and x^2^");
    expect(screen.getByRole("button", { name: "Superscript" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    fireEvent.click(screen.getByRole("button", { name: "Subscript" }));
    expect(e.getMarkdown().trim()).toBe("H~2~O and x~2~");
  });

  it("stays hidden when a right-click selected the word, until the next click or key", () => {
    const { dom, selectFirstWord } = setup();
    // The system selects the word under a right-click, then fires `contextmenu`. The bar must
    // not start to appear in between, or it fades out behind the menu.
    fireEvent.pointerDown(dom, { button: 2 });
    selectFirstWord();
    expect(toolbar()).not.toBeInTheDocument();
    fireEvent.contextMenu(dom);
    expect(toolbar()).not.toBeInTheDocument();

    fireEvent.keyDown(dom, { key: "Shift" });
    selectFirstWord();
    expect(toolbar()).toBeInTheDocument();
  });

  it("comes back after a left click", () => {
    const { dom, selectFirstWord } = setup();
    fireEvent.contextMenu(dom);
    selectFirstWord();
    expect(toolbar()).not.toBeInTheDocument();

    fireEvent.pointerDown(dom, { button: 0 });
    fireEvent.pointerUp(window);
    selectFirstWord();
    expect(toolbar()).toBeInTheDocument();
  });

  describe("synonyms", () => {
    afterEach(() => useLinkResolver.getState().set([], []));

    it("swaps the selected word for a synonym, in its case, keeping it selected", async () => {
      const { e, select } = setup("I feel Happy today");
      select(8, 13);
      // Listed straight away, without taking focus from the editor.
      const glad = await screen.findByRole("button", { name: "Glad" });
      expect(glad).not.toHaveFocus();
      expect(toolbar()).toHaveAttribute("data-side", "below");
      fireEvent.click(glad);
      expect(e.getMarkdown().trim()).toBe("I feel Glad today");
      const { from, to } = e.state.selection;
      expect(e.state.doc.textBetween(from, to)).toBe("Glad");
      // One undo puts the original back.
      act(() => void e.commands.undo());
      expect(e.getMarkdown().trim()).toBe("I feel Happy today");
    });

    it("lists every synonym once as a pill, with no meanings or links", async () => {
      useLinkResolver.getState().set([summary("n_vac", "Vacation")], []);
      const { select } = setup("Planning a holiday soon");
      select(12, 19);
      expect(await screen.findByRole("button", { name: "vacation" })).toBeVisible();
      expect(screen.getByRole("button", { name: "getaway" })).toBeVisible();
      expect(screen.getByText("Replace with")).toBeVisible();
      expect(screen.queryByText(/leisure time/i)).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /Link to/ })).not.toBeInTheDocument();
      const names = screen
        .getAllByRole("button")
        .filter((b) => b.className.includes("nt-synonyms-word"))
        .map((b) => b.textContent);
      expect(new Set(names).size).toBe(names.length);
    });

    it("Esc hides the bar; words with no synonyms list nothing", async () => {
      const { select } = setup("I feel happy and zorbly");
      select(8, 13);
      const glad = await screen.findByRole("button", { name: "glad" });
      fireEvent.keyDown(glad, { key: "Escape" });
      await waitFor(() => expect(toolbar()).not.toBeInTheDocument());

      select(18, 24);
      expect(await screen.findByRole("toolbar")).toBeInTheDocument();
      await waitFor(() => expect(screen.queryByRole("group", { name: /Synonyms/ })).toBeNull());
    });

    it("arrow keys move from the buttons into the list and through it", async () => {
      const { select } = setup("I feel happy");
      select(8, 13);
      const glad = await screen.findByRole("button", { name: "glad" });
      const bold = screen.getByRole("button", { name: "Bold" });
      act(() => bold.focus());
      fireEvent.keyDown(bold, { key: "ArrowDown" });
      expect(glad).toHaveFocus();
      fireEvent.keyDown(glad, { key: "ArrowRight" });
      expect(screen.getByRole("button", { name: "blissful" })).toHaveFocus();
      fireEvent.keyDown(document.activeElement as Element, { key: "End" });
      expect(screen.getByRole("button", { name: "felicitous" })).toHaveFocus();
      fireEvent.keyDown(document.activeElement as Element, { key: "Home" });
      fireEvent.keyDown(document.activeElement as Element, { key: "ArrowUp" });
      expect(bold).toHaveFocus();
      // Focus in the bar keeps it open.
      expect(toolbar()).toBeInTheDocument();
    });

    it("lists no synonyms for long selections", () => {
      const { select } = setup("one two three four five six");
      select(1, 27);
      expect(toolbar()).toBeInTheDocument();
      expect(screen.queryByRole("group", { name: /Synonyms/ })).not.toBeInTheDocument();
    });
  });
});
