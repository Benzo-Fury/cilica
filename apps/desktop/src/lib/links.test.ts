import {
  findUnlinked,
  linkKey,
  parseRefs,
  plainLine,
  refsSignature,
  rewriteWikiLinks,
} from "./links";

const refs = (md: string) => parseRefs(md).map((r) => [r.kind, r.target]);

// The same cases as crates/store/src/links.rs, so the mock and the app agree.
describe("parseRefs", () => {
  it("reads Obsidian links", () => {
    expect(
      refs(
        "See [[Q3 planning]], [[Pricing#Tiers|the tiers]], [[folder/Deep note.md]] and [[Spec#^abc123]].",
      ),
    ).toEqual([
      ["link", "Q3 planning"],
      ["link", "Pricing"],
      ["link", "Deep note"],
      ["link", "Spec"],
    ]);
    expect(refs("![[Onboarding review]] ![[diagram.png]] ![[deck.pdf]]")).toEqual([
      ["embed", "Onboarding review"],
    ]);
    expect(
      refs(
        "[the plan](Q3%20planning.md) [site](https://example.com/a.md) [x](<Other note.md#Top>)",
      ),
    ).toEqual([
      ["link", "Q3 planning"],
      ["link", "Other note"],
    ]);
    expect(refs("[[#Heading in this note]] [[]]")).toEqual([]);
  });

  it("reads tags like Obsidian", () => {
    expect(
      refs(
        "#pricing at the start, mid #area/sub-topic, #2024 is not, C# is not, url https://a.com/#frag is not, #tag/ trims",
      ),
    ).toEqual([
      ["tag", "pricing"],
      ["tag", "area/sub-topic"],
      ["tag", "tag"],
    ]);
    expect(refs("# Heading\n## Another")).toEqual([]);
  });

  it("skips code, comments and frontmatter", () => {
    const md =
      "---\ntitle: x\n---\n```\n[[In code]] #code\n```\nInline `[[code]]` and ``[[two]]``.\n%% [[hidden]] %% shown [[Real]]\n%%\n[[Hidden too]]\n%%\n~~~~\n[[x]]\n~~~\nstill fenced [[y]]\n~~~~\n[[After]]";
    expect(refs(md)).toEqual([
      ["link", "Real"],
      ["link", "After"],
    ]);
  });

  it("dedupes case-insensitively", () => {
    expect(refs("[[Plan]] [[plan]] [[PLAN|x]] #Tag #tag")).toEqual([
      ["link", "Plan"],
      ["tag", "Tag"],
    ]);
  });

  it("tells when a save changed the links", () => {
    expect(refsSignature("Some text [[A]] #t")).toBe(refsSignature("Other words #t [[a]]"));
    expect(refsSignature("[[A]]")).not.toBe(refsSignature("[[B]]"));
  });
});

describe("mentions and renames", () => {
  it("finds a mention outside links, code and URLs", () => {
    const md =
      "The [[Pricing]] page.\n`pricing` in code, prepricing, https://pricing.com\nWe talked pricing today.";
    const hit = findUnlinked(md, "Pricing");
    expect(hit && md.slice(hit.start, hit.end)).toBe("pricing");
    expect(hit?.line).toBe("We talked pricing today.");
    expect(findUnlinked("Only [[Pricing|pricing]] here", "Pricing")).toBeNull();
  });

  it("rewrites links to a renamed note", () => {
    expect(
      rewriteWikiLinks(
        "[[Plan]] and [[plan#Goals|the goals]] and ![[Plan]] [[Planning]]",
        "plan",
        "Q3 plan",
      ),
    ).toBe("[[Q3 plan]] and [[Q3 plan#Goals|the goals]] and ![[Q3 plan]] [[Planning]]");
    expect(rewriteWikiLinks("[[Other]]", "plan", "x")).toBeNull();
  });

  it("makes plain excerpts", () => {
    expect(plainLine("- Agreed on [[Pricing|the pricing]] with **[[Marco Diaz]]**.")).toBe(
      "Agreed on the pricing with Marco Diaz.",
    );
    expect(linkKey("  Q3   Planning ")).toBe("q3 planning");
  });
});
