import { Editor } from "@tiptap/core";
import { afterEach, describe, expect, it } from "vitest";
import { noteExtensions } from "./extensions";
import { matchCase, synonymTarget } from "./synonyms";

let editor: Editor | null = null;
afterEach(() => {
  editor?.destroy();
  editor = null;
});

function selecting(markdown: string, from: number, to: number) {
  editor = new Editor({ extensions: noteExtensions(), content: markdown, contentType: "markdown" });
  editor.commands.setTextSelection({ from, to });
  return synonymTarget(editor.state);
}

describe("which selections get synonyms", () => {
  it("a word or short phrase, without the punctuation around it", () => {
    // "I feel happy, today": "happy, " is 8 to 15.
    expect(selecting("I feel happy, today", 8, 15)).toEqual({ from: 8, to: 13, text: "happy" });
    expect(selecting("Let us look into it", 8, 17)?.text).toBe("look into");
    expect(selecting("A **bold** word", 3, 7)?.text).toBe("bold");
  });

  it("leaves a possessive out, so a synonym keeps it", () => {
    // "Ask Mum's friend": "Mum's" is 5 to 10.
    expect(selecting("Ask Mum's friend", 5, 10)).toEqual({ from: 5, to: 8, text: "Mum" });
    expect(selecting("Ask Mum’s friend", 5, 10)?.text).toBe("Mum");
    // Not a word that only ends in "s".
    expect(selecting("Ask its owner", 5, 8)?.text).toBe("its");
  });

  it("not long selections, code, links, empty or letterless text", () => {
    expect(selecting("one two three four five", 1, 24)).toBeNull();
    expect(selecting("Run `npm test` now", 6, 10)).toBeNull();
    expect(selecting("See [[Cytoplasm]] here", 5, 6)).toBeNull();
    expect(selecting("Costs 2026 dollars", 7, 11)).toBeNull();
    expect(selecting("Two\n\nparagraphs", 1, 9)).toBeNull();
    expect(selecting("Nothing", 3, 3)).toBeNull();
  });
});

describe("matching the selection's case", () => {
  it("keeps capitals and all caps", () => {
    expect(matchCase("happy", "glad")).toBe("glad");
    expect(matchCase("Happy", "glad")).toBe("Glad");
    expect(matchCase("HAPPY", "glad")).toBe("GLAD");
    expect(matchCase("I", "me")).toBe("Me");
    // A synonym that is a name keeps its own capital.
    expect(matchCase("city", "Lisbon")).toBe("Lisbon");
  });
});
