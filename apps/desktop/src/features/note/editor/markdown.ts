/** Markdown helpers around the editor: normalisation for change detection. */

/** Trims trailing whitespace and blank lines so equivalent bodies compare equal. */
export function normalizeMarkdown(markdown: string): string {
  return markdown
    .replace(/\r\n/g, "\n")
    .split("\n")
    .map((line) => line.replace(/[ \t]+$/, ""))
    .join("\n")
    .replace(/^\n+/, "")
    .replace(/\n{3,}/g, "\n\n")
    .trimEnd();
}
