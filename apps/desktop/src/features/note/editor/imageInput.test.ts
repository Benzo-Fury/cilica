import { Editor } from "@tiptap/core";
import { noteExtensions } from "./extensions";
import {
  blockBoundary,
  hasImageExtension,
  type ImageSaver,
  imageFiles,
  insertImageFiles,
} from "./imageInput";
import { normalizeMarkdown } from "./markdown";

const SAVED = "attachments/0123456789abcdef0123456789abcdef.png";

function setup(initial = "Hello") {
  const calls: string[] = [];
  const failures: unknown[] = [];
  let fail = false;
  const saver: ImageSaver = {
    file: async (file) => {
      calls.push(`file:${file.name}`);
      if (fail) throw new Error("nope");
      return { path: SAVED, name: "Pasted" };
    },
    source: async (src) => {
      calls.push(`source:${src}`);
      if (fail) throw new Error("nope");
      return { path: SAVED, name: "Web" };
    },
    failed: (e) => failures.push(e),
  };
  const editor = new Editor({
    extensions: noteExtensions({ images: { saver: () => saver, view: null } }),
    content: initial,
    contentType: "markdown",
  });
  return {
    editor,
    calls,
    failures,
    failNext: () => {
      fail = true;
    },
  };
}

const flush = () => new Promise((r) => setTimeout(r, 0));
const md = (editor: Editor) => normalizeMarkdown(editor.getMarkdown());

beforeAll(() => {
  // jsdom has no object URLs.
  URL.createObjectURL ??= () => "blob:http://test/1";
  URL.revokeObjectURL ??= () => {};
});

describe("image input", () => {
  it("saves a web image that an edit brings in, once", async () => {
    const { editor, calls } = setup();
    editor.commands.insertContent("![Board](https://example.com/board.png)", {
      contentType: "markdown",
    });
    await flush();
    await flush();
    expect(calls).toEqual(["source:https://example.com/board.png"]);
    expect(md(editor)).toContain(`![Board](${SAVED})`);
    editor.destroy();
  });

  it("saves pasted data: images as files", async () => {
    const { editor, calls } = setup();
    editor.commands.insertContent('<img src="data:image/png;base64,iVBORw0KGgo=" alt="Shot">');
    await flush();
    await flush();
    expect(calls).toEqual(["source:data:image/png;base64,iVBORw0KGgo="]);
    expect(md(editor)).toContain(SAVED);
    expect(md(editor)).not.toContain("base64");
    editor.destroy();
  });

  it("leaves web images that were already in the note alone", async () => {
    const { editor, calls } = setup("![Old](https://example.com/old.png)\n\nText");
    editor.commands.insertContentAt(editor.state.doc.content.size, "More");
    await flush();
    expect(calls).toEqual([]);
    expect(md(editor)).toContain("https://example.com/old.png");
    editor.destroy();
  });

  it("keeps the web address when the download fails", async () => {
    const { editor, failures, failNext } = setup();
    failNext();
    editor.commands.insertContent("![Board](https://example.com/board.png)", {
      contentType: "markdown",
    });
    await flush();
    await flush();
    expect(failures).toHaveLength(1);
    expect(md(editor)).toContain("https://example.com/board.png");
    editor.destroy();
  });

  it("shows pasted files at once and points them at the saved file", async () => {
    const { editor, calls } = setup("Hello");
    const file = new File([new Uint8Array([1])], "shot.png", { type: "image/png" });
    insertImageFiles(
      editor.view,
      {
        file: async () => ({ path: SAVED, name: "shot" }),
        source: async () => ({ path: SAVED, name: "" }),
        failed: () => {},
      },
      [file],
      0,
    );
    // In memory only: nothing written yet.
    expect(md(editor)).toBe("Hello");
    await flush();
    expect(md(editor)).toBe(`![shot](${SAVED})\n\nHello`);
    expect(calls).toEqual([]);
    editor.destroy();
  });

  it("removes a pasted file that couldn't be saved", async () => {
    const { editor } = setup("Hello");
    const file = new File([new Uint8Array([1])], "shot.png", { type: "image/png" });
    const failed: unknown[] = [];
    insertImageFiles(
      editor.view,
      {
        file: () => Promise.reject(new Error("too big")),
        source: async () => ({ path: SAVED, name: "" }),
        failed: (e) => failed.push(e),
      },
      [file],
      0,
    );
    await flush();
    expect(failed).toHaveLength(1);
    let images = 0;
    editor.state.doc.descendants((n) => {
      if (n.type.name === "image") images++;
    });
    expect(images).toBe(0);
    editor.destroy();
  });

  it("drops between blocks, never inside a paragraph", () => {
    const { editor } = setup("First paragraph\n\nSecond");
    const inFirst = 3;
    expect(blockBoundary(editor.state, inFirst)).toBe(0);
    const firstEnd = editor.state.doc.child(0).nodeSize;
    expect(blockBoundary(editor.state, firstEnd - 2)).toBe(firstEnd);
    editor.destroy();
  });

  it("recognises image files by type or extension", () => {
    expect(hasImageExtension("Photo.HEIC")).toBe(true);
    expect(hasImageExtension("notes.md")).toBe(false);
    expect(hasImageExtension("png")).toBe(false);
    const list = [
      new File([], "a.png", { type: "image/png" }),
      new File([], "b.txt", { type: "text/plain" }),
      new File([], "c.jpeg", { type: "" }),
    ];
    expect(imageFiles(list as unknown as FileList).map((f) => f.name)).toEqual(["a.png", "c.jpeg"]);
  });
});
