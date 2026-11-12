import { describe, expect, it } from "vitest";
import { localCurrency, money } from "./format";

describe("the swear jar's pretend price", () => {
  it("guesses the currency from the language's region", () => {
    expect(localCurrency("en-GB")).toBe("GBP");
    expect(localCurrency("de-DE")).toBe("EUR");
    expect(localCurrency("en-US")).toBe("USD");
    expect(localCurrency("not a locale")).toBe("USD");
  });

  it("drops the pence on whole amounts", () => {
    expect(money(100, "GBP", "en-GB")).toBe("£1");
    expect(money(2300, "GBP", "en-GB")).toBe("£23");
    expect(money(50, "GBP", "en-GB")).toBe("£0.50");
  });
});
