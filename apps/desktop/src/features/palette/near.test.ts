import { describe, expect, it } from "vitest";
import { closeness, distance } from "@/ipc/mock/near";
import { saidBy, whereSaid } from "./near";

describe("close matches", () => {
  it("counts spelling and spacing as close", () => {
    expect(closeness("road map", "roadmap")).toBe(0);
    expect(closeness("roadmaps", "roadmap")).toBe(1);
    expect(closeness("onboarding", "onbaording")).toBe(1);
    expect(closeness("roadmap", "Roadmap")).toBeNull();
    expect(closeness("the roadmap", "roadmap")).toBeNull();
    expect(closeness("budget", "roadmap")).toBeNull();
    expect(distance("abcd", "abdc")).toBe(1);
  });

  it("words who said it and where", () => {
    expect(saidBy({ speaker: "Priya", is_self: false })).toBe(", said by Priya");
    expect(saidBy({ speaker: "You", is_self: true })).toBe(", which you said");
    expect(saidBy({ speaker: null, is_self: false })).toBe("");
    expect(whereSaid({ meetings: 1, note_title: "Q2 planning" })).toBe("In Q2 planning");
    expect(whereSaid({ meetings: 4, note_title: "Q2 planning" })).toBe("4 meetings");
  });
});
