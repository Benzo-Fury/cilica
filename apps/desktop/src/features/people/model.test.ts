import { describe, expect, it } from "vitest";
import type { Person } from "@/ipc/types";
import { forgetCopy, voiceLine } from "./model";

const person = (patch: Partial<Person>): Person => ({
  id: "p",
  name: "Sam",
  color_index: 3,
  sample_count: 0,
  meeting_count: 2,
  created_at: "",
  is_self: false,
  ...patch,
});

describe("saved voices", () => {
  it("says how well Margin knows a voice", () => {
    expect(voiceLine(person({ sample_count: 3 }))).toBe("3 voice samples · 2 meetings");
    expect(voiceLine(person({}))).toBe("Voice not learnt yet · 2 meetings");
    expect(voiceLine(person({ is_self: true, name: "You" }))).toBe(
      "Margin doesn't know your voice yet · 2 meetings",
    );
  });

  it("words forgetting a voice for you and for others", () => {
    expect(forgetCopy(person({ is_self: true, name: "You" })).title).toBe("Forget your voice?");
    expect(forgetCopy(person({})).title).toBe("Forget Sam's voice?");
  });
});
