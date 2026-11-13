import { Editor } from "@tiptap/core";
import { afterEach, describe, expect, it } from "vitest";
import { noteExtensions } from "./extensions";

let editor: Editor | null = null;
afterEach(() => {
  editor?.destroy();
  editor = null;
});

function mount(markdown: string): HTMLElement {
  const element = document.createElement("div");
  editor = new Editor({
    element,
    extensions: noteExtensions(),
    content: markdown,
    contentType: "markdown",
  });
  return editor.view.dom;
}

describe("Check spelling in the note body", () => {
  it("leaves code blocks, inline code and tags out", () => {
    const dom = mount("Some `inline` code and a #roadmap tag.\n\n```ts\nconst teh = 1;\n```");
    expect(dom.querySelector("pre")?.getAttribute("spellcheck")).toBe("false");
    expect(dom.querySelector("p code")?.getAttribute("spellcheck")).toBe("false");
    expect(dom.querySelector(".nt-tag")?.getAttribute("spellcheck")).toBe("false");
  });

  it("lets prose inherit the setting from <html>", () => {
    const dom = mount("Plain words with a [link](https://example.com).");
    expect(dom.hasAttribute("spellcheck")).toBe(false);
    expect(dom.querySelector("p")?.hasAttribute("spellcheck")).toBe(false);
    expect(dom.querySelector("a")?.hasAttribute("spellcheck")).toBe(false);
  });
});
