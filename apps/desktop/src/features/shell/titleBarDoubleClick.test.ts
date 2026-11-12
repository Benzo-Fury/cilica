import { describe, expect, it } from "vitest";
import { isDragRegion } from "./titleBarDoubleClick";

/** The composed path of a click on `el`: the element, then its ancestors. */
function pathOf(el: Element): Element[] {
  const path: Element[] = [];
  for (let node: Element | null = el; node; node = node.parentElement) path.push(node);
  return path;
}

/** The composed path of a click on the element matching `selector`. */
function at(root: Element, selector: string): Element[] {
  const el = root.querySelector(selector);
  if (!el) throw new Error(`no ${selector}`);
  return pathOf(el);
}

function band(html: string): HTMLElement {
  const root = document.createElement("div");
  root.innerHTML = html;
  return root;
}

describe("title band drag regions", () => {
  it("counts a direct hit on a bare drag region", () => {
    const root = band(`<div data-tauri-drag-region id="b"><span id="t">Title</span></div>`);
    expect(isDragRegion(at(root, "#b"))).toBe(true);
    // A bare attribute covers only the element itself, not its children.
    expect(isDragRegion(at(root, "#t"))).toBe(false);
  });

  it("covers the whole subtree with deep", () => {
    const root = band(`<div data-tauri-drag-region="deep"><span id="t">Title</span></div>`);
    expect(isDragRegion(at(root, "#t"))).toBe(true);
  });

  it("never treats buttons or disabled regions as the title bar", () => {
    const root = band(
      `<div data-tauri-drag-region="deep"><button id="btn"><i id="icon"></i></button><span data-tauri-drag-region="false" id="off"></span></div>`,
    );
    expect(isDragRegion(at(root, "#icon"))).toBe(false);
    expect(isDragRegion(at(root, "#off"))).toBe(false);
  });

  it("ignores clicks outside any drag region", () => {
    const root = band(`<p id="p">Body</p>`);
    expect(isDragRegion(at(root, "#p"))).toBe(false);
  });
});
