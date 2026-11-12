/**
 * Two conveniences for writing Markdown by hand:
 * - pasting plain text that is Markdown inserts it formatted (headings, lists, code blocks,
 *   tables) instead of as literal characters;
 * - typing `[ ] ` or `[x] ` at the start of a bullet turns the list into a task list.
 */
import { Extension, InputRule } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

const BLOCK_SYNTAX = [
  /^#{1,6} \S/m, // heading
  /^```/m, // fenced code
  /^\s*[-*+] \[[ xX]\] /m, // task
  /^\s*[-*+] \S/m, // bullet
  /^\s*\d+[.)] \S/m, // numbered
  /^> \S/m, // quote
  /^\|.+\|\s*$/m, // table row
  /^(-{3,}|\*{3,})\s*$/m, // rule
];
const INLINE_SYNTAX = [
  /\*\*[^*\n]+\*\*/, // bold
  /!?\[[^\]\n]+\]\([^)\n]+\)/, // link or image
  /`[^`\n]+`/, // inline code
  /~~[^~\n]+~~/, // strikethrough
];

/** True when plain text reads as Markdown source rather than ordinary prose. */
export function looksLikeMarkdown(text: string): boolean {
  if (!text.trim()) return false;
  return BLOCK_SYNTAX.some((re) => re.test(text)) || INLINE_SYNTAX.some((re) => re.test(text));
}

export const MarkdownPaste = Extension.create({
  name: "markdownPaste",
  addProseMirrorPlugins() {
    const editor = this.editor;
    return [
      new Plugin({
        key: new PluginKey("markdownPaste"),
        props: {
          handlePaste(view, event) {
            const data = event.clipboardData;
            if (!data) return false;
            // Rich text from another app already carries its formatting.
            if (data.getData("text/html")) return false;
            const text = data.getData("text/plain");
            // Inside a code block, pasted text stays literal.
            if (view.state.selection.$from.parent.type.spec.code) return false;
            if (!looksLikeMarkdown(text) || !editor.markdown) return false;
            const doc = editor.markdown.parse(text);
            if (!doc.content?.length) return false;
            return editor.commands.insertContent(doc.content);
          },
        },
      }),
    ];
  },
});

export const TaskShortcut = Extension.create({
  name: "taskShortcut",
  addInputRules() {
    return [
      new InputRule({
        find: /^\[([ xX])?\]\s$/,
        handler: ({ state, range, match, chain }) => {
          const { $from } = state.selection;
          const item = $from.node(-1);
          const list = $from.node(-2);
          if (item?.type.name !== "listItem" || list?.type.name !== "bulletList") return null;
          const checked = (match[1] ?? " ").toLowerCase() === "x";
          chain()
            .deleteRange(range)
            .toggleTaskList()
            .updateAttributes("taskItem", { checked })
            .run();
        },
      }),
    ];
  },
});
