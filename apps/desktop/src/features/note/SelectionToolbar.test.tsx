import { act, fireEvent, render, screen } from "@testing-library/react";
import { Editor } from "@tiptap/core";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
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

function setup() {
  const element = document.createElement("div");
  document.body.append(element);
  editor = new Editor({
    element,
    extensions: noteExtensions(),
    content: "Thsi is wrnog",
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
  return { dom: e.view.dom, selectFirstWord };
}

const toolbar = () => screen.queryByRole("toolbar");

describe("SelectionToolbar", () => {
  it("shows over a selection", () => {
    const { selectFirstWord } = setup();
    selectFirstWord();
    expect(toolbar()).toBeInTheDocument();
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
});
