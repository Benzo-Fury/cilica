import { describe, expect, it } from "vitest";
import { SWEAR_WORDS } from "@/ipc/mock/swears";
import { SwearMatcher } from "./swears";

const m = new SwearMatcher(SWEAR_WORDS);
const roots = (text: string, mild = true) => m.find(text, mild).map((h) => h.root);

// The same cases as `cilica_domain::swears` tests, so the editor and the backend agree.
describe("SwearMatcher", () => {
  it("finds whole words only", () => {
    expect(roots("Oh, for fuck’s sake. That's bullshit.")).toEqual(["fuck", "shit"]);
    expect(roots("We assess the class in Scunthorpe, hello shell")).toEqual([]);
  });

  it("can leave mild ones out", () => {
    expect(roots("Damn, that was a bloody good shit idea")).toEqual(["damn", "bloody", "shit"]);
    expect(roots("Damn, that was a bloody good shit idea", false)).toEqual(["shit"]);
  });

  it("reads stretched, censored and abbreviated words", () => {
    expect(roots("fuuuuck, shiiiit, f***, s**t, WTF")).toEqual([
      "fuck",
      "shit",
      "fuck",
      "shit",
      "fuck",
    ]);
    expect(roots("f**king hell")).toEqual(["fuck", "hell"]);
    expect(roots("** and *s and x")).toEqual([]);
  });

  it("reports where each word sits", () => {
    const text = "Holy shit, we’re recording";
    const [hit] = m.find(text, true);
    expect(hit && text.slice(hit.start, hit.end)).toBe("shit");
  });

  it("maps every listed word to its root", () => {
    for (const w of SWEAR_WORDS) expect(m.canonical(w.word)?.root).toBe(w.root);
    expect(m.canonical("Motherfucking")).toEqual({ root: "fuck", mild: false });
    expect(m.canonical("pricing")).toBeNull();
  });
});
