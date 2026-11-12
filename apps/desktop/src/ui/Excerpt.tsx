/** Renders a search excerpt whose matches are wrapped in `<mark>…</mark>`, without innerHTML. */
const ENTITIES: Record<string, string> = {
  "&lt;": "<",
  "&gt;": ">",
  "&amp;": "&",
  "&quot;": '"',
  "&#39;": "'",
};
const decodeEntities = (s: string) => s.replace(/&(lt|gt|amp|quot|#39);/g, (m) => ENTITIES[m] ?? m);

export function parseMarked(excerpt: string): { text: string; mark: boolean }[] {
  const parts: { text: string; mark: boolean }[] = [];
  const re = /<mark>([\s\S]*?)<\/mark>/g;
  let last = 0;
  for (let m = re.exec(excerpt); m; m = re.exec(excerpt)) {
    if (m.index > last)
      parts.push({ text: decodeEntities(excerpt.slice(last, m.index)), mark: false });
    parts.push({ text: decodeEntities(m[1] ?? ""), mark: true });
    last = m.index + m[0].length;
  }
  if (last < excerpt.length) parts.push({ text: decodeEntities(excerpt.slice(last)), mark: false });
  return parts;
}

export function Excerpt({ excerpt, className }: { excerpt: string; className?: string }) {
  return (
    <span className={className}>
      {parseMarked(excerpt).map((p, i) =>
        // biome-ignore lint/suspicious/noArrayIndexKey: positional text runs
        p.mark ? <mark key={i}>{p.text}</mark> : <span key={i}>{p.text}</span>,
      )}
    </span>
  );
}
