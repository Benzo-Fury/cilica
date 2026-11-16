/**
 * Note properties: the YAML block (`---` … `---`) at the very top of a note body. The editor never
 * shows it; saves keep it (see crates/domain/src/properties.rs, which this mirrors).
 */

export const RELATED = "related";
export const UNRELATED = "unrelated";
export const ALIASES = "aliases";

/** The property block (without its `---` lines) and the rest; `yaml` is null when there is none. */
export function splitProperties(markdown: string): { yaml: string | null; body: string } {
  const none = { yaml: null, body: markdown };
  if (!markdown.startsWith("---\n")) return none;
  const rest = markdown.slice(4);
  let yaml: string;
  let after: string;
  if (rest.startsWith("---")) {
    yaml = "";
    after = rest.slice(3);
  } else {
    const end = rest.indexOf("\n---");
    if (end < 0) return none;
    yaml = rest.slice(0, end);
    after = rest.slice(end + 4);
  }
  if (!(after === "" || after.startsWith("\n")) || !readsAsYaml(yaml)) return none;
  return { yaml, body: after.replace(/^\n+/, "") };
}

/** The body without its property block: what the editor shows. */
export function bodyOf(markdown: string): string {
  return splitProperties(markdown).body;
}

/** `markdown` with its visible text replaced by `body`, keeping its properties. */
export function withBody(markdown: string, body: string): string {
  const { yaml } = splitProperties(markdown);
  return yaml?.trim() ? joinProperties(yaml, body) : body;
}

export function joinProperties(yaml: string, body: string): string {
  const y = yaml.replace(/^\n+|\n+$/g, "");
  const b = body.replace(/^\n+/, "");
  return y.trim() ? `---\n${y}\n---\n\n${b}` : `---\n---\n${b}`;
}

/** The body a save stores: one without a block keeps the current block; an empty block clears it. */
export function withKeptProperties(current: string, incoming: string): string {
  const next = splitProperties(incoming);
  if (next.yaml !== null) return next.yaml.trim() ? incoming : next.body;
  const { yaml } = splitProperties(current);
  return yaml?.trim() ? joinProperties(yaml, incoming) : incoming;
}

/** The values of a top-level key in any of YAML's list forms, unquoted. */
export function listProperty(yaml: string, key: string): string[] {
  const lines = yaml.split("\n");
  const block = keyBlock(lines, key);
  if (!block) return [];
  const value = (lines[block.start] ?? "")
    .slice((lines[block.start] ?? "").indexOf(":") + 1)
    .trim();
  if (value) {
    if (value.startsWith("[") && value.endsWith("]")) return flowItems(value.slice(1, -1));
    const one = unquote(value);
    return one ? [one] : [];
  }
  return lines
    .slice(block.start + 1, block.end)
    .map((l) => l.trim())
    .filter((l) => l.startsWith("-"))
    .map((l) => unquote(l.slice(1)))
    .filter(Boolean);
}

/** Set a top-level key to a list, in place or at the end; an empty list removes it. */
export function setListProperty(yaml: string, key: string, values: string[]): string {
  const lines = yaml ? yaml.split("\n") : [];
  const block = values.length
    ? [`${key}:`, ...values.map((v) => `  - "${v.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`)]
    : [];
  const at = keyBlock(lines, key);
  const out = at
    ? [...lines.slice(0, at.start), ...block, ...lines.slice(at.end)]
    : [...trimTrailing(lines), ...block];
  return trimTrailing(out).join("\n");
}

/** The note a `[[Title|text]]` or `[[Title#Heading]]` value points at; plain text as is. */
export function linkTarget(value: string): string {
  const v = value.trim();
  const inner = v.startsWith("[[") && v.endsWith("]]") ? v.slice(2, -2) : v;
  return (inner.split("|")[0] ?? "").split("#")[0]?.trim() ?? "";
}

function trimTrailing(lines: string[]): string[] {
  const out = [...lines];
  while (out.length && !out[out.length - 1]?.trim()) out.pop();
  return out;
}

function isKeyLine(line: string): boolean {
  const colon = line.indexOf(":");
  if (colon <= 0) return false;
  const key = line.slice(0, colon);
  return !/^[\s#-]/.test(key) && /^[\p{L}\p{N}_\-. ]+$/u.test(key);
}

function readsAsYaml(yaml: string): boolean {
  const lines = yaml.split("\n").filter((l) => l.trim());
  if (lines.length === 0) return true;
  return isKeyLine(lines[0] ?? "") && lines.every((l) => isKeyLine(l) || /^[\s\-#]/.test(l));
}

function keyBlock(lines: string[], key: string): { start: number; end: number } | null {
  const start = lines.findIndex(
    (l) => isKeyLine(l) && l.slice(0, l.indexOf(":")).trim().toLowerCase() === key.toLowerCase(),
  );
  if (start < 0) return null;
  let end = start + 1;
  while (end < lines.length && (/^[\s-]/.test(lines[end] ?? "") || !lines[end]?.trim())) end++;
  while (end > start + 1 && !lines[end - 1]?.trim()) end--;
  return { start, end };
}

function flowItems(inner: string): string[] {
  const items: string[] = [];
  let current = "";
  let quote: string | null = null;
  let depth = 0;
  for (let i = 0; i < inner.length; i++) {
    const c = inner[i] as string;
    if (quote) {
      current += c;
      if (c === "\\" && quote === '"' && i + 1 < inner.length) current += inner[++i];
      else if (c === quote) quote = null;
    } else if (c === '"' || c === "'") {
      quote = c;
      current += c;
    } else if (c === "[") {
      depth++;
      current += c;
    } else if (c === "]") {
      depth = Math.max(0, depth - 1);
      current += c;
    } else if (c === "," && depth === 0) {
      items.push(current);
      current = "";
    } else current += c;
  }
  items.push(current);
  return items.map(unquote).filter(Boolean);
}

function unquote(value: string): string {
  const v = value.trim();
  if (v.length >= 2 && v.startsWith('"') && v.endsWith('"'))
    return v.slice(1, -1).replace(/\\"/g, '"').replace(/\\\\/g, "\\");
  if (v.length >= 2 && v.startsWith("'") && v.endsWith("'"))
    return v.slice(1, -1).replace(/''/g, "'");
  return v;
}

/** A note's `aliases`, trimmed, without blanks or repeats (ignoring case): what the store sends. */
export function noteAliases(markdown: string): string[] {
  const { yaml } = splitProperties(markdown);
  if (!yaml) return [];
  const seen = new Set<string>();
  return listProperty(yaml, ALIASES)
    .map((a) => a.split(/\s+/).filter(Boolean).join(" "))
    .filter((a) => {
      const key = a.toLowerCase();
      if (!a || seen.has(key)) return false;
      seen.add(key);
      return true;
    });
}
