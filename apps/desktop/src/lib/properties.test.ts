import { describe, expect, it } from "vitest";
import {
  bodyOf,
  joinProperties,
  linkTarget,
  listProperty,
  setListProperty,
  splitProperties,
  withBody,
  withKeptProperties,
} from "./properties";

describe("note properties", () => {
  it("splits and joins like the store", () => {
    const md = joinProperties('related:\n  - "[[Pricing]]"', "Hello");
    expect(md).toBe('---\nrelated:\n  - "[[Pricing]]"\n---\n\nHello');
    expect(splitProperties(md)).toEqual({ yaml: 'related:\n  - "[[Pricing]]"', body: "Hello" });
    expect(bodyOf("Just text")).toBe("Just text");
    expect(splitProperties("---\n---\nBody")).toEqual({ yaml: "", body: "Body" });
  });

  it("leaves a rule around a paragraph alone", () => {
    const md = "---\n\nHello there\n\n---\n\nMore";
    expect(splitProperties(md)).toEqual({ yaml: null, body: md });
  });

  it("keeps properties when the editor saves", () => {
    const current = '---\nrelated:\n  - "[[A]]"\n---\n\nOld';
    expect(withKeptProperties(current, "New")).toBe('---\nrelated:\n  - "[[A]]"\n---\n\nNew');
    expect(withKeptProperties(current, "---\n---\nNew")).toBe("New");
    expect(withBody(current, "New")).toBe(withKeptProperties(current, "New"));
    expect(withBody("Plain", "New")).toBe("New");
  });

  it("reads and writes lists", () => {
    const yaml = 'aliases: SSO\ntags: [a, "b, c"]\nrelated:\n  - "[[One, two]]"\nsource: x';
    expect(listProperty(yaml, "aliases")).toEqual(["SSO"]);
    expect(listProperty(yaml, "Tags")).toEqual(["a", "b, c"]);
    expect(listProperty(yaml, "related")).toEqual(["[[One, two]]"]);
    const next = setListProperty(yaml, "related", ["[[One, two]]", "[[B]]"]);
    expect(next).toBe(
      'aliases: SSO\ntags: [a, "b, c"]\nrelated:\n  - "[[One, two]]"\n  - "[[B]]"\nsource: x',
    );
    expect(setListProperty(next, "related", [])).toBe('aliases: SSO\ntags: [a, "b, c"]\nsource: x');
    expect(setListProperty("", "unrelated", ["Q3"])).toBe('unrelated:\n  - "Q3"');
    expect(linkTarget("[[Pricing#Tiers|x]]")).toBe("Pricing");
  });
});
