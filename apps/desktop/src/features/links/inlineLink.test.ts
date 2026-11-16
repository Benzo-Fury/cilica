import { describe, expect, it } from "vitest";
import { linkKey } from "@/lib/links";
import { buildTitleIndex, matchTitleBefore, singulars } from "./inlineLink";

function index(...titles: string[]) {
  return buildTitleIndex(new Map(titles.map((t, i) => [linkKey(t), { id: `n${i}`, title: t }])));
}

describe("spotting a note title as you type", () => {
  const titles = index(
    "Cytoplasm",
    "Cell",
    "Cell membrane",
    "Single sign-on",
    "Q3 plan",
    "AI",
    "2026",
    "Mitochondria",
    "Lisbon",
    "Strawberry",
    "Sandwich",
    "Sign-on flow",
    "McDonald's",
    "Ideas",
    "Meeting",
  );

  it("matches the word just finished, whatever its case", () => {
    expect(matchTitleBefore("The cytoplasm", titles)).toMatchObject({
      title: "Cytoplasm",
      typed: "cytoplasm",
      length: 9,
    });
    expect(matchTitleBefore("CYTOPLASM", titles)?.title).toBe("Cytoplasm");
  });

  it("prefers the longest title ending at the caret", () => {
    expect(matchTitleBefore("the cell", titles)?.title).toBe("Cell");
    expect(matchTitleBefore("the cell membrane", titles)?.title).toBe("Cell membrane");
    expect(matchTitleBefore("we shipped single sign-on", titles)?.title).toBe("Single sign-on");
    expect(matchTitleBefore("our q3 plan", titles)?.title).toBe("Q3 plan");
  });

  it("only matches whole words", () => {
    expect(matchTitleBefore("endocytoplasm", titles)).toBeNull();
    expect(matchTitleBefore("membrane", titles)).toBeNull();
    expect(matchTitleBefore("cytoplasmic", titles)).toBeNull();
  });

  it("looks past opening punctuation", () => {
    expect(matchTitleBefore("(cytoplasm", titles)).toMatchObject({ typed: "cytoplasm" });
    expect(matchTitleBefore("“Mitochondria", titles)).toMatchObject({ typed: "Mitochondria" });
  });

  it("ignores titles that are too short or have no letters", () => {
    expect(matchTitleBefore("ask the AI", titles)).toBeNull();
    expect(matchTitleBefore("in 2026", titles)).toBeNull();
  });

  it("falls back to a shorter title when the longer one is turned down", () => {
    const skip = (key: string) => key === "cell membrane";
    expect(matchTitleBefore("cell membrane", titles, { skip })).toBeNull();
    expect(matchTitleBefore("the cell", titles, { skip: (key) => key === "cell" })).toBeNull();
  });

  it("does nothing after whitespace or with no titles", () => {
    expect(matchTitleBefore("cytoplasm ", titles)).toBeNull();
    expect(matchTitleBefore("cytoplasm", index())).toBeNull();
    expect(matchTitleBefore("", titles)).toBeNull();
  });

  it("finds plurals, and says the link needs the title", () => {
    expect(matchTitleBefore("two cytoplasms", titles)).toMatchObject({
      title: "Cytoplasm",
      key: "cytoplasm",
      typed: "cytoplasms",
      exact: false,
    });
    expect(matchTitleBefore("fresh strawberries", titles)?.title).toBe("Strawberry");
    expect(matchTitleBefore("three sandwiches", titles)?.title).toBe("Sandwich");
    expect(matchTitleBefore("the cell membranes", titles)?.title).toBe("Cell membrane");
    expect(matchTitleBefore("the cytoplasm", titles)?.exact).toBe(true);
  });

  it("keeps a possessive outside the link", () => {
    expect(matchTitleBefore("Lisbon's", titles)).toMatchObject({
      typed: "Lisbon",
      trail: 2,
      exact: true,
    });
    expect(matchTitleBefore("Lisbon’s", titles)).toMatchObject({ typed: "Lisbon", trail: 2 });
    expect(matchTitleBefore("the cells'", titles)).toMatchObject({ typed: "cells", trail: 1 });
    // A title that itself ends in 's is matched whole.
    expect(matchTitleBefore("lunch at McDonald's", titles)).toMatchObject({
      title: "McDonald's",
      trail: 0,
    });
  });

  it("reads hyphens and spaces alike", () => {
    expect(matchTitleBefore("the sign on flow", titles)).toMatchObject({
      title: "Sign-on flow",
      typed: "sign on flow",
      exact: false,
    });
    expect(matchTitleBefore("the sign-on flow", titles)?.exact).toBe(true);
  });

  it("does not undo endings that are not plurals", () => {
    expect(singulars("cytoplasms")).toContain("cytoplasm");
    expect(singulars("glass")).toEqual([]);
    expect(singulars("status")).toEqual([]);
    expect(singulars("analysis")).toEqual([]);
    expect(singulars("bus")).toEqual([]);
    expect(singulars("gas")).toEqual([]);
  });

  it("offers an everyday one-word title only when it looks meant", () => {
    // Just the word in a sentence: no.
    expect(matchTitleBefore("I had some ideas", titles)).toBeNull();
    expect(matchTitleBefore("Ideas", titles)).toBeNull(); // a capital that starts a sentence
    expect(matchTitleBefore("Done. Meeting", titles)).toBeNull();
    expect(matchTitleBefore("long meetings", titles)).toBeNull();
    // A capital mid-sentence: yes.
    expect(matchTitleBefore("it is in my Ideas", titles)?.title).toBe("Ideas");
    expect(matchTitleBefore("see (Meeting", titles)?.title).toBe("Meeting");
    // A title you have linked from a hint before: yes.
    expect(matchTitleBefore("some ideas", titles, { trusted: (k) => k === "ideas" })?.title).toBe(
      "Ideas",
    );
    // Not everyday: always.
    expect(matchTitleBefore("Cytoplasm", titles)?.title).toBe("Cytoplasm");
  });

  it("spots aliases and links them to the note's title", () => {
    const withAliases = buildTitleIndex(
      new Map([
        ["single sign-on", { id: "sso", title: "Single sign-on", aliases: ["SSO", "login flow"] }],
        ["login flow", { id: "flow", title: "Login flow", aliases: [] }],
        ["trip to lisbon", { id: "lis", title: "Trip to Lisbon", aliases: ["Lisbon trip", "AI"] }],
      ]),
    );
    expect(matchTitleBefore("we shipped SSO", withAliases)).toMatchObject({
      noteId: "sso",
      title: "Single sign-on",
      key: "single sign-on",
      typed: "SSO",
      exact: false,
    });
    // Plurals and possessives work for aliases too.
    expect(matchTitleBefore("both Lisbon trips", withAliases)?.title).toBe("Trip to Lisbon");
    expect(matchTitleBefore("SSO's", withAliases)).toMatchObject({ typed: "SSO", trail: 2 });
    // Another note's title wins over an alias.
    expect(matchTitleBefore("the login flow", withAliases)?.noteId).toBe("flow");
    // Too short to spot, like a title.
    expect(matchTitleBefore("ask the AI", withAliases)).toBeNull();
    // Turning the note down turns its aliases down too.
    expect(
      matchTitleBefore("SSO", withAliases, { skip: (key) => key === "single sign-on" }),
    ).toBeNull();
  });

  it("stays fast with a large library", () => {
    const many = index(
      ...Array.from({ length: 20_000 }, (_, i) => `Topic number ${i} with words`),
      "Cytoplasm",
    );
    const line = "Some ordinary sentence about the cell wall and the cytoplasm";
    const t0 = performance.now();
    for (let i = 0; i < 1000; i++) matchTitleBefore(line, many);
    const perWord = (performance.now() - t0) / 1000;
    expect(matchTitleBefore(line, many)?.title).toBe("Cytoplasm");
    // One frame is 16 ms; a word end should cost a tiny fraction of that.
    expect(perWord).toBeLessThan(0.5);
  });
});
