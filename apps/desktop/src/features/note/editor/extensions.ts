/**
 * The note editor's schema. Markdown is the stored format, so every node and mark here must
 * round-trip through @tiptap/markdown (see markdown.test.ts): headings 1–6, bold, italic,
 * strikethrough, superscript `^x^`, subscript `~x~`, inline code, links, images, quotes, rules, bullet / numbered / task lists,
 * tables, fenced code blocks with syntax colouring, and Obsidian's `[[links]]` and `#tags`.
 */
import type { AnyExtension, NodeViewRenderer } from "@tiptap/core";
import { CodeBlockLowlight } from "@tiptap/extension-code-block-lowlight";
import Placeholder from "@tiptap/extension-placeholder";
import { TableKit } from "@tiptap/extension-table";
import TaskItem from "@tiptap/extension-task-item";
import TaskList from "@tiptap/extension-task-list";
import { Markdown } from "@tiptap/markdown";
import StarterKit from "@tiptap/starter-kit";
import { common, createLowlight } from "lowlight";
import { FindInNote } from "./findInNote";
import { NoteImage } from "./image";
import { ImageInput, type ImageSaver } from "./imageInput";
import {
  type InlineLinkHandlers,
  InlineLinkHintExtension,
  type InlineLinkMatcher,
} from "./inlineLinkHint";
import { MarkdownPaste, TaskShortcut } from "./markdownInput";
import { Subscript, Superscript } from "./scripts";
import { StreamHighlight } from "./streamHighlight";
import { type OnWordEnd, SwearWatch } from "./swearWatch";
import { Tags } from "./tags";
import { WikiLink, type WikiLinkOptions } from "./wikiLink";
import { type SuggestHandlers, WikiSuggest } from "./wikiSuggest";

const lowlight = createLowlight(common);

export interface NoteExtensionOptions {
  placeholder?: string;
  onWordEnd?: OnWordEnd;
  /** How `[[links]]` resolve and open. Without it links still round-trip, unstyled. */
  links?: Partial<WikiLinkOptions>;
  /** Who renders `[[` suggestions. */
  suggest?: () => SuggestHandlers | null;
  onOpenTag?: (tag: string) => void;
  /** Offer to link a note title as soon as you finish typing it. Off without a matcher. */
  linkHint?: { match: InlineLinkMatcher; handlers: () => InlineLinkHandlers | null };
  /** Saves pasted, dropped and web images as attachments. Without it they stay as they came. */
  images?: { saver: () => ImageSaver | null; view: NodeViewRenderer | null };
}

export function noteExtensions(options: NoteExtensionOptions = {}): AnyExtension[] {
  return [
    StarterKit.configure({
      heading: { levels: [1, 2, 3, 4, 5, 6] },
      // Replaced below by the block that colours its code.
      codeBlock: false,
      // Markdown has no underline; keep the schema to what the file format can store.
      underline: false,
      link: { openOnClick: false, autolink: true, linkOnPaste: true, defaultProtocol: "https" },
      trailingNode: false,
      // Where a dragged image or file will land.
      dropcursor: { class: "nt-dropcursor", color: false, width: 2 },
      // Code is not prose: Check spelling leaves it alone.
      code: { HTMLAttributes: { spellcheck: "false" } },
    }),
    CodeBlockLowlight.configure({
      lowlight,
      defaultLanguage: null,
      HTMLAttributes: { spellcheck: "false" },
    }),
    Superscript,
    Subscript,
    NoteImage.configure({ view: options.images?.view ?? null }),
    ImageInput.configure({ saver: options.images?.saver ?? (() => null) }),
    TaskList,
    TaskItem.configure({ nested: true }),
    TableKit.configure({ table: { resizable: false } }),
    Markdown.configure({ indentation: { style: "space", size: 2 } }),
    Placeholder.configure({ placeholder: options.placeholder ?? "Start writing" }),
    MarkdownPaste,
    TaskShortcut,
    FindInNote,
    StreamHighlight,
    SwearWatch.configure({ onWordEnd: options.onWordEnd ?? null }),
    WikiLink.configure(options.links ?? {}),
    WikiSuggest.configure({ handlers: options.suggest ?? (() => null) }),
    Tags.configure({ onOpen: options.onOpenTag ?? null }),
    InlineLinkHintExtension.configure({
      match: options.linkHint?.match ?? null,
      handlers: options.linkHint?.handlers ?? (() => null),
    }),
  ];
}
