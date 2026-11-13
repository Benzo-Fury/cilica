import { describe, expect, it } from "vitest";
import { decisions, overtime, phraseCount, TicCounter } from "@/ipc/mock/faces";
import type { HomeGlance } from "@/ipc/types";
import {
  andList,
  availableFaces,
  duration,
  firstName,
  hourLabel,
  jarFaceReady,
  minSec,
  numberWord,
  openingFace,
  paceChange,
  pickFace,
  possessive,
  quietStretch,
  savesPerDot,
  talkHeadline,
  timecode,
} from "./glance";

const EMPTY: HomeGlance = {
  tics: null,
  quotes: [],
  talk: null,
  pace: null,
  silence: null,
  own_lines: [],
  writing: null,
  questions: [],
  overtime: null,
  slow_decision: null,
  monologue: null,
  bare_note: null,
  mid_sentence: null,
  spelling: null,
  ghostwriter: null,
  undos: null,
  swear_jar: null,
};
const TICS: HomeGlance["tics"] = {
  you: { phrase: "basically", count: 4, name: null, color_index: null },
  others: [],
};

describe("home faces", () => {
  it("offers only the faces the data can fill", () => {
    expect(availableFaces(undefined)).toEqual(["doodle"]);
    expect(availableFaces({ ...EMPTY, tics: TICS })).toEqual(["tics"]);
    expect(
      availableFaces({ ...EMPTY, mid_sentence: 3, ghostwriter: { cilica_pct: 20, words: 900 } }),
    ).toEqual(["mid_sentence", "ghostwriter"]);
  });

  it("never repeats the last face when there is a choice", () => {
    const faces = availableFaces({
      ...EMPTY,
      tics: TICS,
      quotes: [
        {
          note_id: "n",
          note_title: "Q3",
          held_at: "2026-11-11T09:00:00Z",
          at_ms: 0,
          text: "Hi",
          speaker: null,
          color_index: null,
        },
      ],
    });
    for (const roll of [0, 0.4, 0.99]) {
      expect(pickFace(faces, "tics", roll)).not.toBe("tics");
    }
    expect(pickFace(["doodle"], "doodle", 0.5)).toBe("doodle");
  });

  it("keeps the chosen face, with another standing in until it has data", () => {
    expect(openingFace(["tics", "pace"], "pace", "pace", 0.1)).toBe("pace");
    expect(openingFace(["tics", "pace"], "silence", null, 0.1)).toBe("tics");
    expect(openingFace(["tics", "pace"], "random", "tics", 0.1)).toBe("pace");
  });

  it("shows the doodle only when chosen or when nothing else can show", () => {
    expect(openingFace(["tics", "pace"], "doodle", null, 0.1)).toBe("doodle");
    expect(openingFace(["doodle"], "random", null, 0.5)).toBe("doodle");
    expect(openingFace(["doodle"], "silence", null, 0.5)).toBe("doodle");
    for (const roll of [0, 0.5, 0.99]) {
      expect(openingFace(["tics", "pace"], "random", null, roll)).not.toBe("doodle");
    }
  });

  it("words the talk share, pace and durations", () => {
    const slice = (name: string | null, ms: number, is_self = false) => ({
      name,
      color_index: 1,
      ms,
      is_self,
    });
    expect(
      talkHeadline({
        total_ms: 400,
        meetings: 2,
        slices: [slice("Priya", 250), slice("You", 150, true)],
      }),
    ).toEqual({
      lead: "You did",
      strong: "38%",
      tail: "of the talking this week",
    });
    expect(
      talkHeadline({
        total_ms: 400,
        meetings: 1,
        slices: [slice("Priya", 250), slice("Marco", 150)],
      }).strong,
    ).toBe("Priya");
    expect(paceChange({ you_wpm: 162, others_wpm: 141, last_week_wpm: 148 })).toBe(
      "14 faster than last week",
    );
    expect(paceChange({ you_wpm: 150, others_wpm: null, last_week_wpm: 149 })).toBe(
      "about the same as last week",
    );
    expect(paceChange({ you_wpm: 150, others_wpm: null, last_week_wpm: null })).toBeNull();
    expect(duration(45_000)).toBe("45 s");
    expect(duration(15_120_000)).toBe("4 h 12 min");
  });

  it("words the hours you write in", () => {
    expect([0, 9, 12, 21].map(hourLabel)).toEqual(["12 am", "9 am", "12 pm", "9 pm"]);
    const hours = Array.from({ length: 24 }, (_, h) => (h >= 1 && h < 7 ? 0 : 1));
    expect(quietStretch(hours)).toBe("Quiet from 1 am to 7 am.");
    // Quiet that wraps past midnight.
    const night = Array.from({ length: 24 }, (_, h) => (h >= 22 || h < 5 ? 0 : 2));
    expect(quietStretch(night)).toBe("Quiet from 10 pm to 5 am.");
    expect(quietStretch(Array.from({ length: 24 }, () => 1))).toBeNull();
  });

  it("gives each dot more saves only when the busiest hour would not fit", () => {
    const hours = (most: number) => Array.from({ length: 24 }, (_, h) => (h === 17 ? most : 1));
    expect(savesPerDot(hours(6))).toBe(1);
    expect(savesPerDot(hours(10))).toBe(1);
    expect(savesPerDot(hours(11))).toBe(2);
    expect(savesPerDot(hours(95))).toBe(10);
    expect(savesPerDot(Array.from({ length: 24 }, () => 0))).toBe(1);
  });

  it("writes recording positions as timecodes", () => {
    expect(timecode(862_000)).toBe("14:22");
    expect(timecode(3_725_000)).toBe("1:02:05");
  });
});

describe("words for the newer faces", () => {
  it("formats turns, lists and names", () => {
    expect(minSec(252_000)).toBe("4 min 12 s");
    expect(minSec(45_000)).toBe("45 s");
    expect(minSec(360_000)).toBe("6 min");
    expect(andList([41, 44, 47])).toBe("41, 44 and 47");
    expect(numberWord(12)).toBe("twelve");
    expect(numberWord(31)).toBe("31");
    expect(possessive("Marco")).toBe("Marco's");
    expect(possessive("James")).toBe("James'");
  });
});

describe("mock face counts", () => {
  it("counts filler phrases on whole words, repeats included", () => {
    expect(phraseCount(["to", "be", "fair", "to", "be", "fair"], "to be fair")).toBe(2);
    const t = new TicCounter();
    t.add("You", true, 1, "Basically, basically. Basically!");
    t.add("Marco", false, 2, "Actually fine. Factually.");
    expect(t.result()).toEqual({
      you: { phrase: "basically", count: 3, name: null, color_index: null },
      others: [],
    });
  });

  it("reads decisions and finds overtime against the usual length", () => {
    expect(
      decisions("## Decisions\n- Ship it [14:02]\n- Keep it\n\n## Action items\n- [ ] x [1:00]"),
    ).toEqual({
      count: 2,
      at: [842_000],
    });
    const min = 60_000;
    const day = 86_400_000;
    const h = (d: number, m: number) => ({
      note_id: `n${d}`,
      title: "Q3 sync",
      held: d * day,
      ms: m * min,
    });
    const o = overtime([h(0, 30), h(7, 31), h(14, 41), h(21, 42)], 20 * day);
    expect(o).toMatchObject({ usual_ms: 31 * min, actual_ms: 42 * min, again: true });
    expect(overtime([h(0, 30), h(21, 42)], 20 * day)).toBeNull();
  });
});

describe("settings preview samples", () => {
  it("fills only the parts with no real numbers", async () => {
    const { withSamples, SAMPLE_GLANCE } = await import("./sample");
    expect(withSamples(undefined)).toEqual({ glance: SAMPLE_GLANCE, sampled: true });
    const mixed = withSamples({ ...EMPTY, tics: TICS });
    expect(mixed.glance.tics).toBe(TICS);
    expect(mixed.glance.pace).toBe(SAMPLE_GLANCE.pace);
    expect(mixed.sampled).toBe(true);
  });
});

describe("swear jar faces", () => {
  const thursday = new Date(2026, 10, 12, 10);
  const monday = new Date(2026, 10, 9, 10);

  it("show the jar from three coins and Who's paying only with someone else in a meeting", async () => {
    const { sampleJar } = await import("./sample");
    const full = sampleJar(23, 17);
    expect(jarFaceReady("swear_jar", full)).toBe(true);
    expect(jarFaceReady("whos_paying", full)).toBe(true);
    expect(jarFaceReady("swear_jar", { ...full, count: 2 })).toBe(false);
    const alone = {
      ...full,
      has_meetings: false,
      people: [{ name: null, color_index: null, count: 23 }],
    };
    expect(jarFaceReady("whos_paying", alone)).toBe(false);
    expect(availableFaces({ ...EMPTY, swear_jar: full })).toEqual(["swear_jar", "whos_paying"]);
  });

  it("show a clean week from Thursday, after a week with coins", async () => {
    const { sampleJar } = await import("./sample");
    const clean = sampleJar(0, 17);
    expect(jarFaceReady("clean_week", clean, { now: thursday })).toBe(true);
    expect(jarFaceReady("clean_week", clean, { now: monday })).toBe(false);
    expect(jarFaceReady("clean_week", clean, { now: monday, anyDay: true })).toBe(true);
    expect(jarFaceReady("clean_week", { ...clean, last_week: 0 }, { now: thursday })).toBe(false);
    expect(jarFaceReady("clean_week", null)).toBe(false);
  });

  it("call people by their first name", () => {
    expect(firstName("Priya Shah")).toBe("Priya");
    expect(firstName("Marco")).toBe("Marco");
    expect(firstName("Person 2")).toBe("Person 2");
  });
});

describe("mock swear jar", () => {
  it("counts the seed's meeting and typed coins, with a history", async () => {
    const { createState } = await import("@/ipc/mock/seed");
    const { swearJar } = await import("@/ipc/mock/swears");
    const state = createState("demo");
    const jar = swearJar(state, state.settings.swear_jar);
    // Q3: "fuck’s sake" and "bloody" from you, "shit" from Marco; six typed coins this week.
    expect(jar.count).toBe(9);
    expect(jar.people.map((p) => [p.name, p.count])).toEqual([
      [null, 8],
      ["Marco Diaz", 1],
      ["Priya Shah", 0],
      ["Speaker 3", 0],
    ]);
    expect(jar.last_week).toBe(17);
    expect(jar.weeks.map((w) => w.count)).toEqual([41, 18, 26, 9, 31, 14, 20, 17, 9]);
    expect(jar.record?.count).toBe(41);
    const strict = swearJar(state, { ...state.settings.swear_jar, notes: false });
    expect(strict.count).toBe(3);
  });
});
