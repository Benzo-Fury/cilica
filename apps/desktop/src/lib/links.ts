/**
 * Obsidian-style links in note Markdown: `[[Title]]`, `[[Title|shown]]`, `[[Title#Heading]]`,
 * `![[Title]]` embeds, `[text](Title.md)` and `#tags`. The Rust store has the canonical parser
 * (crates/store/src/links.rs); this one mirrors it for the in-browser mock and for cheap checks in
 * the UI (did a save change any links?).
 */

export type RefKind = "link" | "embed" | "tag";

export interface ParsedRef {
  kind: RefKind;
  /** As written: a note title (no path, heading or alias) or a tag (no `#`). */
  target: string;
  key: string;
}

/** How titles, names and tags compare: trimmed, whitespace folded, lowercased. */
export function linkKey(text: string): string {
  return text.trim().split(/\s+/).filter(Boolean).join(" ").toLowerCase();
}

const ATTACHMENT =
  /\.(png|jpe?g|gif|webp|svg|bmp|avif|heic|pdf|mp3|m4a|wav|ogg|flac|webm|mp4|mov|mkv|canvas|excalidraw|base|zip|csv|docx|xlsx|pptx|txt)$/i;

/** The note a wiki link's inside points at, or null for same-note headings and attachments. */
export function wikiPage(inner: string): string | null {
  const target = (inner.split("|")[0] ?? "").replace(/\\$/, "");
  const page = target.split("#")[0] ?? "";
  return notePage(page);
}

function notePage(path: string): string | null {
  const name = (path.split(/[/\\]/).pop() ?? "").trim().replace(/\.md$/i, "").trim();
  if (!name || ATTACHMENT.test(name)) return null;
  return name;
}

/** Parts of a wiki link's inside: `Title#Heading|shown` → { page, anchor, alias }. */
export function splitWiki(inner: string): { page: string; anchor: string; alias: string | null } {
  const bar = inner.indexOf("|");
  const target = bar >= 0 ? inner.slice(0, bar) : inner;
  const alias = bar >= 0 ? inner.slice(bar + 1) : null;
  const hash = target.indexOf("#");
  return {
    page: (hash >= 0 ? target.slice(0, hash) : target).trim(),
    anchor: hash >= 0 ? target.slice(hash) : "",
    alias,
  };
}

const TAG_CHAR = /[\p{L}\p{N}_\-/]/u;

function isTag(tag: string): boolean {
  return (
    tag.length > 0 &&
    !tag.startsWith("/") &&
    [...tag].every((c) => TAG_CHAR.test(c)) &&
    [...tag].some((c) => !/\p{N}/u.test(c))
  );
}

interface Found {
  start: number;
  end: number;
  ref: ParsedRef | null;
}

/** Links, tags and protected ranges (code, comments, URLs) on one line. */
function scanLine(line: string, state: { comment: boolean }): Found[] {
  const found: Found[] = [];
  let commentStart: number | null = state.comment ? 0 : null;
  let i = 0;
  while (i < line.length) {
    if (line.startsWith("%%", i)) {
      if (commentStart !== null) {
        found.push({ start: commentStart, end: i + 2, ref: null });
        commentStart = null;
        state.comment = false;
      } else {
        commentStart = i;
        state.comment = true;
      }
      i += 2;
      continue;
    }
    if (commentStart !== null) {
      i++;
      continue;
    }
    const c = line[i];
    if (c === "\\") {
      i += 2;
      continue;
    }
    if (c === "`") {
      let run = 0;
      while (line[i + run] === "`") run++;
      const ticks = "`".repeat(run);
      const close = line.indexOf(ticks, i + run);
      const end = close >= 0 ? close + run : i + run;
      found.push({ start: i, end, ref: null });
      i = end;
      continue;
    }
    const embed = line.startsWith("![[", i);
    if (embed || line.startsWith("[[", i)) {
      const open = embed ? i + 1 : i;
      const close = line.indexOf("]]", open + 2);
      const inner = close >= 0 ? line.slice(open + 2, close) : "";
      if (close >= 0 && inner && !inner.includes("[[")) {
        const page = wikiPage(inner);
        found.push({
          start: i,
          end: close + 2,
          ref: page ? { kind: embed ? "embed" : "link", target: page, key: linkKey(page) } : null,
        });
        i = close + 2;
        continue;
      }
    }
    if (c === "[") {
      const m = /^\[([^\]]*)\]\(([^)]*)\)/.exec(line.slice(i));
      if (m) {
        const page = mdLinkPage(m[2] ?? "");
        found.push({
          start: i,
          end: i + m[0].length,
          ref: page ? { kind: "link", target: page, key: linkKey(page) } : null,
        });
        i += m[0].length;
        continue;
      }
    }
    if (line.startsWith("http://", i) || line.startsWith("https://", i)) {
      const space = line.slice(i).search(/\s/);
      const end = space >= 0 ? i + space : line.length;
      found.push({ start: i, end, ref: null });
      i = end;
      continue;
    }
    if (c === "#" && (i === 0 || /\s/.test(line[i - 1] ?? ""))) {
      let j = i + 1;
      while (j < line.length && TAG_CHAR.test(line[j] ?? "")) j++;
      const tag = line.slice(i + 1, j).replace(/\/+$/, "");
      if (isTag(tag)) {
        found.push({
          start: i,
          end: i + 1 + tag.length,
          ref: { kind: "tag", target: tag, key: tag.toLowerCase() },
        });
        i += 1 + tag.length;
        continue;
      }
    }
    i++;
  }
  if (commentStart !== null) found.push({ start: commentStart, end: line.length, ref: null });
  return found;
}

function mdLinkPage(dest: string): string | null {
  const d = dest.trim().replace(/^</, "").replace(/>$/, "").split(' "')[0]?.trim() ?? "";
  if (/:\/\/|^#|^mailto:|^data:/.test(d)) return null;
  const path = d.split("#")[0] ?? "";
  if (!/\.md$/i.test(path)) return null;
  try {
    return notePage(decodeURIComponent(path));
  } catch {
    return notePage(path);
  }
}

/** Calls `fn` for each line outside fenced code and frontmatter. */
function eachLine(markdown: string, fn: (line: string, state: { comment: boolean }) => void) {
  let text = markdown.replace(/\r\n/g, "\n");
  if (text.startsWith("---\n")) {
    const end = text.indexOf("\n---", 4);
    if (end >= 0 && (text.length === end + 4 || text[end + 4] === "\n")) {
      text = text.slice(end + 5);
    }
  }
  let fence: string | null = null;
  const state = { comment: false };
  for (const line of text.split("\n")) {
    const m = /^ {0,3}(`{3,}|~{3,})/.exec(line);
    if (m?.[1]) {
      if (fence === null) fence = m[1];
      else if (m[1][0] === fence[0] && m[1].length >= fence.length) fence = null;
      continue;
    }
    if (fence === null) fn(line, state);
  }
}

/** Every link, embed and tag in a body, first of each kept. */
export function parseRefs(markdown: string): ParsedRef[] {
  const out: ParsedRef[] = [];
  const seen = new Set<string>();
  eachLine(markdown, (line, state) => {
    for (const f of scanLine(line, state)) {
      if (!f.ref?.key) continue;
      const id = `${f.ref.kind}:${f.ref.key}`;
      if (seen.has(id)) continue;
      seen.add(id);
      out.push(f.ref);
    }
  });
  return out;
}

/** A compact fingerprint of a body's links and tags, to tell whether a save changed them. */
export function refsSignature(markdown: string): string {
  return parseRefs(markdown)
    .map((r) => `${r.kind}:${r.key}`)
    .sort()
    .join("\n");
}

/** The first line linking to `key`, or null. */
export function linkingLine(markdown: string, key: string): string | null {
  let hit: string | null = null;
  eachLine(markdown, (line, state) => {
    if (hit !== null) return;
    if (scanLine(line, state).some((f) => f.ref && f.ref.kind !== "tag" && f.ref.key === key)) {
      hit = line;
    }
  });
  return hit;
}

/** The first place `title` appears as a whole word outside links, code and tags. */
export function findUnlinked(
  markdown: string,
  title: string,
): { start: number; end: number; line: string } | null {
  const needle = title.toLowerCase();
  if (!needle.trim()) return null;
  const lines = markdown.split("\n");
  let offset = 0;
  let fence: string | null = null;
  const state = { comment: false };
  for (const line of lines) {
    const lineStart = offset;
    offset += line.length + 1;
    const m = /^ {0,3}(`{3,}|~{3,})/.exec(line);
    if (m?.[1]) {
      if (fence === null) fence = m[1];
      else if (m[1][0] === fence[0] && m[1].length >= fence.length) fence = null;
      continue;
    }
    if (fence !== null) continue;
    const protectedRanges = scanLine(line, state);
    const lower = line.toLowerCase();
    let from = 0;
    while (from <= lower.length) {
      const at = lower.indexOf(needle, from);
      if (at < 0) break;
      const end = at + needle.length;
      from = at + 1;
      const before = line[at - 1] ?? "";
      const after = line[end] ?? "";
      if (/[\p{L}\p{N}]/u.test(before) || /[\p{L}\p{N}]/u.test(after)) continue;
      if (protectedRanges.some((f) => end > f.start && at < f.end)) continue;
      return { start: lineStart + at, end: lineStart + end, line };
    }
  }
  return null;
}

/** Point wiki links to `oldKey` at `newTitle`, keeping headings and shown text. */
export function rewriteWikiLinks(
  markdown: string,
  oldKey: string,
  newTitle: string,
): string | null {
  let changed = false;
  const out = markdown.replace(
    /(!?)\[\[([^[\]\n]+?)\]\]/g,
    (whole, bang: string, inner: string) => {
      const { page, anchor, alias } = splitWiki(inner);
      if (linkKey(notePage(page) ?? "") !== oldKey) return whole;
      changed = true;
      return `${bang}[[${newTitle}${anchor}${alias !== null ? `|${alias}` : ""}]]`;
    },
  );
  return changed ? out : null;
}

/** A line of Markdown as plain text, for excerpts. */
export function plainLine(line: string): string {
  return line
    .replace(/!?\[\[([^[\]\n]+?)\]\]/g, (_, inner: string) => {
      const { page, alias } = splitWiki(inner);
      return alias ?? page.split("/").pop() ?? page;
    })
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/^\s*(?:[-*+]\s+|\d+[.)]\s+|>\s*|#{1,6}\s+)*(?:\[[ xX]\]\s+)?/, "")
    .replace(/[*_`~]|==/g, "")
    .replace(/\s+/g, " ")
    .trim();
}
