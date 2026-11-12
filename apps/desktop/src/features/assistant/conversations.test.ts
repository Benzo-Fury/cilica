import { describe, expect, it } from "vitest";
import type { Thread } from "@/ipc/types";
import { conversationMeta, groupConversations, retentionLine } from "./conversations";

const now = new Date(2026, 10, 12, 15, 0);

function thread(id: string, updated: Date): Thread {
  const iso = updated.toISOString();
  return { id, title: id, created_at: iso, updated_at: iso };
}

describe("groupConversations", () => {
  it("puts threads under Today, Yesterday and Earlier and skips empty groups", () => {
    const groups = groupConversations(
      [
        thread("a", new Date(2026, 10, 12, 9, 0)),
        thread("b", new Date(2026, 10, 11, 23, 0)),
        thread("c", new Date(2026, 10, 8, 10, 0)),
      ],
      now,
    );
    expect(groups.map((g) => [g.label, g.threads.map((t) => t.id)])).toEqual([
      ["Today", ["a"]],
      ["Yesterday", ["b"]],
      ["Earlier", ["c"]],
    ]);
    expect(
      groupConversations([thread("a", new Date(2026, 10, 1))], now).map((g) => g.label),
    ).toEqual(["Earlier"]);
  });
});

describe("conversationMeta", () => {
  it("says when retention removes a conversation today or tomorrow", () => {
    expect(conversationMeta(thread("x", new Date(2026, 10, 6, 10, 0)), 7, now)).toBe(
      "Deleted tomorrow",
    );
    expect(conversationMeta(thread("x", new Date(2026, 10, 5, 18, 0)), 7, now)).toBe(
      "Deleted today",
    );
  });

  it("otherwise shows when it was last active, and never expires when retention is off", () => {
    expect(conversationMeta(thread("x", new Date(2026, 10, 11, 10, 0)), 7, now)).toBe("Yesterday");
    expect(conversationMeta(thread("x", new Date(2026, 10, 5, 18, 0)), 0, now)).not.toMatch(
      /Deleted/,
    );
  });
});

describe("retentionLine", () => {
  it("names the period, or says nothing is removed", () => {
    expect(retentionLine(7)).toBe("Conversations are deleted after 7 days.");
    expect(retentionLine(1)).toBe("Conversations are deleted after 1 day.");
    expect(retentionLine(0)).toBe("Conversations are kept until you delete them.");
  });
});
