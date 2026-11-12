import { describe, expect, it } from "vitest";
import { ownLines } from "./written";

/** Each quotable line with who said it (null for your own words). */
const saidBy = (body: string) => ownLines(body).map(([text, , who]) => [text, who]);

describe("ownLines attribution", () => {
  it("keeps the speaker of a quote, in every common shape", () => {
    const sam = (text: string) => [[text, "Sam"]];
    expect(saidBy('"We should ship before the pricing change." — Sam')).toEqual(
      sam("We should ship before the pricing change."),
    );
    expect(saidBy("“We should ship before the pricing change,” said Sam.")).toEqual(
      sam("We should ship before the pricing change"),
    );
    expect(saidBy('"We should ship before the pricing change," Sam said.')).toEqual(
      sam("We should ship before the pricing change"),
    );
    expect(saidBy('- Sam: "We should ship before the pricing change."')).toEqual(
      sam("We should ship before the pricing change."),
    );
    expect(saidBy("We should ship before the pricing change. — Sam")).toEqual(
      sam("We should ship before the pricing change."),
    );
    expect(saidBy("> We should ship before the pricing change.\n> — Sam Lee")).toEqual([
      ["We should ship before the pricing change.", "Sam Lee"],
    ]);
  });

  it("leaves your own words as yours", () => {
    const mine = (body: string) => saidBy(body).every(([, who]) => who === null);
    expect(mine('I think "good enough" should be the bar for v1.')).toBe(true);
    expect(mine('Note: "we should ship before the pricing change."')).toBe(true);
    expect(mine('"We should ship before the pricing change," I said.')).toBe(true);
    expect(mine("We should ship before the pricing change - the team agrees.")).toBe(true);
  });
});
