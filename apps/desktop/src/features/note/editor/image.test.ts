import { Editor } from "@tiptap/core";
import { NodeSelection } from "@tiptap/pm/state";
import { noteExtensions } from "./extensions";
import {
  formatImageAlt,
  moveImageTo,
  moveSelectedImage,
  needsSaving,
  parseImageAlt,
} from "./image";
import { normalizeMarkdown } from "./markdown";

const FILE = "attachments/0123456789abcdef0123456789abcdef.png";

function editorWith(markdown: string) {
  return new Editor({ extensions: noteExtensions(), content: markdown, contentType: "markdown" });
}

function roundTrip(markdown: string): string {
  const editor = editorWith(markdown);
  const out = normalizeMarkdown(editor.getMarkdown());
  editor.destroy();
  return out;
}

function imagePos(editor: Editor, index = 0): number {
  const found: number[] = [];
  editor.state.doc.descendants((node, pos) => {
    if (node.type.name === "image") found.push(pos);
  });
  return found[index] as number;
}

describe("image alt text", () => {
  it.each([
    ["Whiteboard", { alt: "Whiteboard", align: null, width: null }],
    ["Whiteboard|400", { alt: "Whiteboard", align: null, width: 400 }],
    ["Whiteboard|center|400", { alt: "Whiteboard", align: "center", width: 400 }],
    ["Whiteboard|right", { alt: "Whiteboard", align: "right", width: null }],
    ["Whiteboard|left|320", { alt: "Whiteboard", align: null, width: 320 }],
    ["|300", { alt: "", align: null, width: 300 }],
    ["pic|300x200", { alt: "pic", align: null, width: 300 }],
    ["A|B", { alt: "A|B", align: null, width: null }],
    ["center", { alt: "center", align: null, width: null }],
    ["", { alt: "", align: null, width: null }],
  ])("reads %j", (raw, expected) => {
    expect(parseImageAlt(raw)).toEqual(expected);
  });

  it("writes placement and width in Obsidian's order, leaving out left", () => {
    expect(formatImageAlt("Board", "center", 400)).toBe("Board|center|400");
    expect(formatImageAlt("Board", "left", 400)).toBe("Board|400");
    expect(formatImageAlt(null, null, null)).toBe("");
    expect(formatImageAlt("A [b]", "right", null)).toBe("A b|right");
  });
});

describe("image Markdown", () => {
  it.each([
    `![Board](${FILE})`,
    `![Board|center|400](${FILE})`,
    `![Board|right](${FILE} "The whiteboard after the call")`,
    `![|240](${FILE})`,
    "![Whiteboard](https://example.com/board.png)",
  ])("keeps %j exactly", (markdown) => {
    expect(roundTrip(markdown)).toBe(markdown);
  });

  it("reads the caption, placement and width onto the node", () => {
    const editor = editorWith(`![Board|center|400](${FILE} "Caption")`);
    const node = editor.state.doc.nodeAt(imagePos(editor));
    expect(node?.attrs).toMatchObject({
      src: FILE,
      alt: "Board",
      align: "center",
      width: 400,
      title: "Caption",
    });
    editor.destroy();
  });

  it("escapes quotes in captions", () => {
    const editor = editorWith(`![Board](${FILE})`);
    const pos = imagePos(editor);
    editor.view.dispatch(editor.state.tr.setNodeAttribute(pos, "title", 'She said "ship it"'));
    const out = normalizeMarkdown(editor.getMarkdown());
    expect(out).toBe(`![Board](${FILE} "She said \\"ship it\\"")`);
    expect(roundTrip(out)).toBe(out);
    editor.destroy();
  });

  it("never writes an image that only exists in memory", () => {
    const editor = editorWith("Before\n\nAfter");
    const type = editor.schema.nodes.image as NonNullable<typeof editor.schema.nodes.image>;
    editor.view.dispatch(
      editor.state.tr.insert(0, type.create({ src: "blob:http://x/1", pending: "k" })),
    );
    expect(normalizeMarkdown(editor.getMarkdown())).toBe("Before\n\nAfter");
    editor.destroy();
  });

  it("keeps a web image's address while its copy downloads", () => {
    const editor = editorWith("![Board](https://example.com/board.png)");
    const pos = imagePos(editor);
    editor.view.dispatch(editor.state.tr.setNodeAttribute(pos, "pending", "k"));
    expect(normalizeMarkdown(editor.getMarkdown())).toBe("![Board](https://example.com/board.png)");
    editor.destroy();
  });

  it("knows which sources still have to be saved", () => {
    expect(needsSaving("https://example.com/a.png")).toBe(true);
    expect(needsSaving("data:image/png;base64,AAAA")).toBe(true);
    expect(needsSaving(FILE)).toBe(false);
    expect(needsSaving("blob:http://x/1")).toBe(false);
    expect(needsSaving(null)).toBe(false);
  });
});

describe("moving images", () => {
  const doc = `First\n\n![A](${FILE})\n\nSecond\n\nThird`;

  it("moves the selected image up and down a block with ⌥↑ / ⌥↓", () => {
    const editor = editorWith(doc);
    editor.view.dispatch(
      editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, imagePos(editor))),
    );
    expect(moveSelectedImage(editor.state, 1, editor.view.dispatch)).toBe(true);
    expect(normalizeMarkdown(editor.getMarkdown())).toBe(
      `First\n\nSecond\n\n![A](${FILE})\n\nThird`,
    );
    expect(moveSelectedImage(editor.state, -1, editor.view.dispatch)).toBe(true);
    expect(moveSelectedImage(editor.state, -1, editor.view.dispatch)).toBe(true);
    expect(normalizeMarkdown(editor.getMarkdown())).toBe(
      `![A](${FILE})\n\nFirst\n\nSecond\n\nThird`,
    );
    // Already at the top.
    expect(moveSelectedImage(editor.state, -1, editor.view.dispatch)).toBe(false);
    editor.destroy();
  });

  it("does nothing when the selection is not an image", () => {
    const editor = editorWith(doc);
    expect(moveSelectedImage(editor.state, 1)).toBe(false);
    editor.destroy();
  });

  it("drops a dragged image before another block, or at the end", () => {
    const editor = editorWith(doc);
    const from = imagePos(editor);
    const end = editor.state.doc.content.size;
    const tr = moveImageTo(editor.state, from, end);
    expect(tr).not.toBeNull();
    editor.view.dispatch(tr as NonNullable<typeof tr>);
    expect(normalizeMarkdown(editor.getMarkdown())).toBe(
      `First\n\nSecond\n\nThird\n\n![A](${FILE})`,
    );
    // Dropping it where it already is changes nothing.
    const at = imagePos(editor);
    expect(moveImageTo(editor.state, at, at)).toBeNull();
    editor.destroy();
  });
});
