/**
 * A small, safe Markdown reader for display (assistant replies, diff cards, history). It never
 * produces HTML: it splits a document into blocks and inline runs that components render as
 * React elements, so raw HTML in the text stays text.
 */

export type BlockKind =
  | "heading"
  | "paragraph"
  | "bullet"
  | "ordered"
  | "task"
  | "quote"
  | "code"
  | "table"
  | "rule";

export interface Block {
  kind: BlockKind;
  /** The exact source, including the blank lines that follow it; blocks concatenate to the document. */
  raw: string;
  /** The block's text with its Markdown markers removed (inline marks kept). */
  text: string;
  /** Heading level (1–6). */
  level?: number;
  /** Task items. */
  checked?: boolean;
  /** Ordered items: the number written in the source. */
  number?: number;
  /** List nesting depth, 0 for top level. */
  depth?: number;
}

const HEADING = /^(#{1,6})\s+(.*)$/;
const TASK = /^(\s*)[-*+]\s+\[([ xX])\]\s+(.*)$/;
const BULLET = /^(\s*)[-*+]\s+(.*)$/;
const ORDERED = /^(\s*)(\d+)[.)]\s+(.*)$/;
const FENCE = /^\s*(```|~~~)/;
const RULE = /^\s*([-*_])(\s*\1){2,}\s*$/;
const TABLE = /^\s*\|/;
const QUOTE = /^\s*>\s?(.*)$/;

const isBlank = (line: string) => line.trim() === "";

function startsBlock(line: string): boolean {
  return (
    HEADING.test(line) ||
    BULLET.test(line) ||
    ORDERED.test(line) ||
    FENCE.test(line) ||
    RULE.test(line) ||
    TABLE.test(line) ||
    QUOTE.test(line)
  );
}

/** Split Markdown into blocks. Joining every `raw` gives back the input exactly. */
export function splitBlocks(markdown: string): Block[] {
  const source = markdown.replace(/\r\n/g, "\n");
  const lines = source.split("\n");
  // `split` leaves an empty last element for a trailing newline; keep newlines on each line.
  const withNl = lines.map((l, i) => (i < lines.length - 1 ? `${l}\n` : l));
  if (withNl[withNl.length - 1] === "") withNl.pop();
  /** Line `n` without its newline (patterns with `$` must not see it). */
  const at = (n: number) => (withNl[n] ?? "").replace(/\n$/, "");
  const blocks: Block[] = [];
  let leading = "";
  let i = 0;
  const take = (count: number) => {
    const raw = withNl.slice(i, i + count).join("");
    i += count;
    // Blank lines after a block belong to it.
    let tail = "";
    while (i < withNl.length && isBlank(withNl[i] ?? "")) {
      tail += withNl[i];
      i += 1;
    }
    return raw + tail;
  };
  // Blank lines before the first block attach to it.
  while (i < withNl.length && isBlank(withNl[i] ?? "")) {
    leading += withNl[i];
    i += 1;
  }
  while (i < withNl.length) {
    const start = i;
    const line = at(i);
    let block: Block;
    const heading = line.match(HEADING);
    const task = line.match(TASK);
    const bullet = line.match(BULLET);
    const ordered = line.match(ORDERED);
    if (FENCE.test(line)) {
      const fence = line.trim().slice(0, 3);
      let end = i + 1;
      while (end < withNl.length && !(withNl[end] ?? "").trim().startsWith(fence)) end += 1;
      const count = Math.min(withNl.length, end + 1) - i;
      const body = withNl.slice(i + 1, i + count - 1).join("");
      block = { kind: "code", raw: take(count), text: body.replace(/\n$/, "") };
    } else if (heading) {
      block = {
        kind: "heading",
        level: heading[1]?.length ?? 1,
        raw: take(1),
        text: (heading[2] ?? "").replace(/\s+#+\s*$/, ""),
      };
    } else if (RULE.test(line)) {
      block = { kind: "rule", raw: take(1), text: "" };
    } else if (task) {
      block = {
        kind: "task",
        depth: Math.floor((task[1]?.length ?? 0) / 2),
        checked: (task[2] ?? " ").toLowerCase() === "x",
        raw: "",
        text: task[3] ?? "",
      };
      block.raw = take(continuation(i));
    } else if (bullet) {
      block = {
        kind: "bullet",
        depth: Math.floor((bullet[1]?.length ?? 0) / 2),
        raw: "",
        text: bullet[2] ?? "",
      };
      block.raw = take(continuation(i));
    } else if (ordered) {
      block = {
        kind: "ordered",
        depth: Math.floor((ordered[1]?.length ?? 0) / 2),
        number: Number(ordered[2]),
        raw: "",
        text: ordered[3] ?? "",
      };
      block.raw = take(continuation(i));
    } else if (TABLE.test(line)) {
      let end = i;
      while (end < withNl.length && TABLE.test(at(end))) end += 1;
      const rows = withNl
        .slice(i, end)
        .map((r) => r.trim())
        .filter((r) => !/^\|?\s*:?-{3,}/.test(r))
        .map((r) =>
          r
            .replace(/^\||\|$/g, "")
            .split("|")
            .map((c) => c.trim())
            .join(" · "),
        );
      block = { kind: "table", raw: take(end - i), text: rows.join("\n") };
    } else if (QUOTE.test(line)) {
      let end = i;
      const parts: string[] = [];
      while (end < withNl.length && QUOTE.test(at(end))) {
        parts.push(at(end).match(QUOTE)?.[1] ?? "");
        end += 1;
      }
      block = { kind: "quote", raw: take(Math.max(1, end - i)), text: parts.join(" ") };
    } else {
      let end = i + 1;
      while (end < withNl.length && !isBlank(at(end)) && !startsBlock(at(end))) {
        end += 1;
      }
      const text = withNl
        .slice(i, end)
        .map((l) => l.trim())
        .join(" ");
      block = { kind: "paragraph", raw: take(end - i), text };
    }
    // Never loop on a line no rule consumed.
    if (i === start) block.raw += take(1);
    if (leading) {
      block.raw = leading + block.raw;
      leading = "";
    }
    blocks.push(block);
  }
  if (leading) blocks.push({ kind: "paragraph", raw: leading, text: "" });
  return blocks;

  /** A list item plus its indented continuation lines. */
  function continuation(start: number): number {
    const indent = ((withNl[start] ?? "").match(/^\s*/)?.[0].length ?? 0) + 2;
    let end = start + 1;
    while (end < withNl.length) {
      const next = (withNl[end] ?? "").replace(/\n$/, "");
      if (isBlank(next)) break;
      const nextIndent = next.match(/^\s*/)?.[0].length ?? 0;
      if (nextIndent < indent || startsBlock(next)) break;
      end += 1;
    }
    return end - start;
  }
}

// ───────── Inline ─────────

export type Inline =
  | { type: "text"; text: string }
  | { type: "strong"; children: Inline[] }
  | { type: "em"; children: Inline[] }
  | { type: "code"; text: string }
  | { type: "link"; text: string; href: string }
  /** Margin cites notes as ("Title", 12:34). */
  | { type: "cite"; title: string; time: string | null };

const INLINE =
  /(`[^`\n]+`)|(\*\*[^*\n]+\*\*|__[^_\n]+__)|(\*[^*\s][^*\n]*\*|_[^_\s][^_\n]*_)|(\[[^\]\n]+\]\([^)\s]+\))|(\("[^"\n]{1,120}"(?:,\s*\d{1,2}:\d{2}(?::\d{2})?)?\))/;

/** Parse inline Markdown: code, strong, emphasis, links and citations. Anything else is text. */
export function parseInline(text: string): Inline[] {
  const out: Inline[] = [];
  let rest = text;
  while (rest) {
    const m = rest.match(INLINE);
    if (!m || m.index === undefined) {
      out.push({ type: "text", text: rest });
      break;
    }
    if (m.index > 0) out.push({ type: "text", text: rest.slice(0, m.index) });
    const token = m[0];
    if (m[1]) out.push({ type: "code", text: token.slice(1, -1) });
    else if (m[2]) out.push({ type: "strong", children: parseInline(token.slice(2, -2)) });
    else if (m[3]) out.push({ type: "em", children: parseInline(token.slice(1, -1)) });
    else if (m[4]) {
      const link = token.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
      out.push({ type: "link", text: link?.[1] ?? token, href: link?.[2] ?? "" });
    } else if (m[5]) {
      const cite = token.match(/^\("([^"]+)"(?:,\s*([\d:]+))?\)$/);
      out.push({ type: "cite", title: cite?.[1] ?? "", time: cite?.[2] ?? null });
    }
    rest = rest.slice(m.index + token.length);
  }
  // Merge neighbouring text runs.
  return out.reduce<Inline[]>((acc, node) => {
    const prev = acc[acc.length - 1];
    if (node.type === "text" && prev?.type === "text") prev.text += node.text;
    else acc.push(node);
    return acc;
  }, []);
}

/** Inline nodes as plain text (marks removed, links and citations as their words). */
export function nodesText(nodes: readonly Inline[]): string {
  return nodes
    .map((node) => {
      switch (node.type) {
        case "text":
        case "code":
          return node.text;
        case "strong":
        case "em":
          return nodesText(node.children);
        case "link":
          return node.text;
        default:
          return `("${node.title}"${node.time ? `, ${node.time}` : ""})`;
      }
    })
    .join("");
}

/** Inline Markdown as plain text. */
export function inlineText(text: string): string {
  return nodesText(parseInline(text));
}

/** A block as one line of readable prose, the way the note shows it (no Markdown markers). */
export function blockProse(block: Block): string {
  switch (block.kind) {
    case "code":
    case "table":
      return block.text;
    case "rule":
      return "";
    default:
      return inlineText(block.text);
  }
}

/** Seconds from a "12:34" or "1:02:03" timecode. */
export function timecodeMs(time: string): number | null {
  const parts = time.split(":").map(Number);
  if (parts.some((n) => Number.isNaN(n))) return null;
  return parts.reduce((acc, n) => acc * 60 + n, 0) * 1000;
}
