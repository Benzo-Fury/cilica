import { describe, expect, it } from "vitest";
import { splitBlocks } from "./markdown";
import { diffStats, documentRows, finalMarkdown, proseDiff } from "./proseDiff";

const BEFORE = `## Summary

The team met to plan the beta.

## Action items

- [ ] Book usability sessions
- [ ] Send the pricing deck
`;

const AFTER = `## Summary

The team met to plan the beta launch on the 24th.

## Action items

- [ ] Book five usability sessions for step 3 before Thursday
- [ ] Send the pricing deck
- [ ] Priya to share the rollout plan on Thursday
`;

describe("splitBlocks", () => {
  it("round-trips the source exactly", () => {
    for (const md of [
      BEFORE,
      AFTER,
      "",
      "one\ntwo\n\n\n- a\n  more\n- b",
      "```js\nx\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
    ]) {
      expect(
        splitBlocks(md)
          .map((b) => b.raw)
          .join(""),
      ).toBe(md);
    }
  });

  it("reads headings, tasks and paragraphs", () => {
    const kinds = splitBlocks(BEFORE).map((b) => b.kind);
    expect(kinds).toEqual(["heading", "paragraph", "heading", "task", "task"]);
  });
});

describe("proseDiff", () => {
  it("finds one hunk per changed region with its section", () => {
    const diff = proseDiff(BEFORE, AFTER);
    expect(diff.hunks).toHaveLength(3);
    expect(diff.hunks.map((h) => h.section)).toEqual(["Summary", "Action items", "Action items"]);
    // The paragraph keeps its words and marks only the change.
    const parts = diff.hunks[0]?.rows[0]?.parts ?? [];
    expect(parts.filter((p) => p.added).map((p) => p.value.trim())).toEqual(["launch on the 24th"]);
    // Readable prose, no Markdown markers.
    expect(diff.hunks[1]?.rows[0]?.parts.map((p) => p.value).join("")).not.toContain("- [ ]");
  });

  it("rebuilds the body from the accepted hunks", () => {
    const diff = proseDiff(BEFORE, AFTER);
    expect(finalMarkdown(diff, new Set([0, 1, 2]))).toBe(AFTER);
    const onlyNew = finalMarkdown(diff, new Set([2]));
    expect(onlyNew).toContain("The team met to plan the beta.\n");
    expect(onlyNew).toContain("- [ ] Book usability sessions\n- [ ] Send the pricing deck\n");
    expect(onlyNew).toContain("- [ ] Priya to share the rollout plan on Thursday");
    expect(onlyNew).not.toContain("five usability");
    const none = finalMarkdown(diff, new Set());
    expect(none.trim()).toBe(BEFORE.trim());
  });

  it("keeps a blank line where a kept paragraph meets an accepted one", () => {
    const diff = proseDiff("One.\n\nTwo.", "One.\n\nTwo changed.\n\nThree.");
    const md = finalMarkdown(diff, new Set([0]));
    expect(md).toBe("One.\n\nTwo changed.\n\nThree.");
    expect(splitBlocks(finalMarkdown(diff, new Set())).map((b) => b.text)).toEqual([
      "One.",
      "Two.",
    ]);
  });

  it("lists every block for history with stats", () => {
    const rows = documentRows(proseDiff(BEFORE, AFTER));
    expect(rows[0]).toMatchObject({ kind: "heading", change: "same" });
    const stats = diffStats(rows);
    expect(stats.added).toBeGreaterThan(5);
    expect(stats.removed).toBe(0);
  });
});
