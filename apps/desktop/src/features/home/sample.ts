/**
 * Made-up numbers for the Settings preview, so every face can be seen before there are meetings
 * to count. Real numbers replace each part as soon as there are some.
 */
import type { HomeGlance } from "@/ipc/types";

export const SAMPLE_GLANCE: HomeGlance = {
  tics: {
    you: { phrase: "basically", count: 37, name: null, color_index: null },
    others: [
      { phrase: "to be fair", count: 23, name: "Marco", color_index: 7 },
      { phrase: "sounds good", count: 17, name: "Priya", color_index: 2 },
    ],
  },
  quotes: [
    {
      note_id: "sample",
      note_title: "Q3 planning sync",
      held_at: "2026-11-10T09:00:00Z",
      at_ms: 862_000,
      text: "If we lose them at email verification, nothing after it matters.",
      speaker: "Marco",
      color_index: 2,
    },
  ],
  talk: {
    total_ms: 15_120_000,
    meetings: 6,
    slices: [
      { name: "You", color_index: 1, ms: 6_200_000, is_self: true },
      { name: "Priya", color_index: 3, ms: 5_000_000, is_self: false },
      { name: "Marco", color_index: 2, ms: 3_920_000, is_self: false },
    ],
  },
  pace: { you_wpm: 162, others_wpm: 141, last_week_wpm: 148 },
  silence: {
    note_id: "sample",
    note_title: "Q3 planning sync",
    held_at: "2026-11-10T09:00:00Z",
    gap_ms: 11_000,
    at_ms: 915_000,
    before: "Who signs off on the new empty state copy?",
    speaker: "Priya",
    color_index: 3,
    is_self: false,
  },
  own_lines: [
    {
      note_id: "sample",
      note_title: "Product thoughts",
      written_at: "2026-10-09T21:10:00Z",
      text: "I think the onboarding problem is really a pricing problem.",
      said_by: null,
    },
  ],
  writing: {
    hours: [0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 2, 1, 1, 2, 1, 1, 2, 2, 3, 5, 7, 9, 6, 2],
    saves: 50,
    peak_hour: 21,
  },
  questions: [
    {
      note_id: "sample",
      note_title: "Pricing ideas",
      written_at: "2026-10-22T19:00:00Z",
      text: "Do we need a free tier at all?",
      said_by: null,
    },
    {
      note_id: "sample",
      note_title: "Weekly 1:1 with Sam",
      written_at: "2026-11-09T09:30:00Z",
      text: "What would make Sam stay another year?",
      said_by: null,
    },
  ],
  overtime: {
    note_id: "sample",
    title: "1:1 with Sam",
    usual_ms: 30 * 60_000,
    actual_ms: 47 * 60_000,
    recent_ms: [41, 44, 39, 47].map((m) => m * 60_000),
    again: true,
  },
  slow_decision: {
    slowest: {
      note_id: "sample",
      title: "Q3 planning sync",
      held_at: "2026-11-11T09:00:00Z",
      duration_ms: 54 * 60_000,
      decisions: 1,
      decided_at_ms: [51 * 60_000],
    },
    fastest: {
      note_id: "sample",
      title: "1:1 with Sam",
      held_at: "2026-11-12T09:00:00Z",
      duration_ms: 20 * 60_000,
      decisions: 4,
      decided_at_ms: [],
    },
  },
  monologue: {
    note_id: "sample",
    note_title: "Q3 planning sync",
    at_ms: 862_000,
    your_ms: 252_000,
    average_ms: 18_000,
    record_name: "Marco",
    record_ms: 390_000,
  },
  bare_note: { note_id: "sample", title: "Big idea", created_at: "2026-10-02T10:00:00Z" },
  mid_sentence: 11,
  spelling: { word: "definitely", wrong: "definately", count: 14 },
  ghostwriter: { margin_pct: 41, words: 12_400 },
  swear_jar: sampleJar(23, 17),
  undos: {
    count: 12,
    latest: {
      note_id: null,
      summary: "Rewrote your intro",
      margin_text: "This quarter, we will strategically prioritise onboarding synergies.",
      your_text: "We're fixing onboarding first.",
    },
  },
};

/** Real numbers where there are some, the samples elsewhere, and whether any sample was used. */
export function withSamples(real: HomeGlance | undefined): {
  glance: HomeGlance;
  sampled: boolean;
} {
  const S = SAMPLE_GLANCE;
  const glance: HomeGlance = {
    tics: real?.tics ?? S.tics,
    quotes: real?.quotes.length ? real.quotes : S.quotes,
    talk: real?.talk ?? S.talk,
    pace: real?.pace ?? S.pace,
    silence: real?.silence ?? S.silence,
    own_lines: real?.own_lines.length ? real.own_lines : S.own_lines,
    writing: real?.writing ?? S.writing,
    questions: real?.questions.length ? real.questions : S.questions,
    overtime: real?.overtime ?? S.overtime,
    slow_decision: real?.slow_decision ?? S.slow_decision,
    monologue: real?.monologue ?? S.monologue,
    bare_note: real?.bare_note ?? S.bare_note,
    mid_sentence: real?.mid_sentence ?? S.mid_sentence,
    spelling: real?.spelling ?? S.spelling,
    ghostwriter: real?.ghostwriter ?? S.ghostwriter,
    undos: real?.undos ?? S.undos,
    swear_jar: real?.swear_jar ?? S.swear_jar,
  };
  const sampled =
    !real?.quotes.length ||
    !real.own_lines.length ||
    !real.questions.length ||
    (
      [
        "tics",
        "talk",
        "pace",
        "silence",
        "writing",
        "overtime",
        "slow_decision",
        "monologue",
        "bare_note",
        "mid_sentence",
        "spelling",
        "ghostwriter",
        "undos",
      ] as const
    ).some((k) => !real[k]);
  return { glance, sampled };
}

/** A sample jar: this week's count, last week's, and Marco and Priya from the sample meeting. */
export function sampleJar(count: number, lastWeek: number): NonNullable<HomeGlance["swear_jar"]> {
  const you = Math.round(count * 0.65);
  const weeks = [41, 18, 26, 9, 31, 14, 20, lastWeek, count];
  const start = "2026-11-09T00:00:00Z";
  const week = (i: number) =>
    new Date(Date.parse(start) - (weeks.length - 1 - i) * 7 * 86_400_000).toISOString();
  return {
    count,
    last_week: lastWeek,
    people: count
      ? [
          { name: null, color_index: null, count: you },
          { name: "Marco Diaz", color_index: 2, count: count - you },
          { name: "Priya Shah", color_index: 6, count: 0 },
        ]
      : [],
    words: count
      ? [
          { word: "fuck", count: Math.ceil(count * 0.4) },
          { word: "shit", count: Math.floor(count * 0.3) },
        ]
      : [],
    weeks: weeks.map((n, i) => ({ start: week(i), count: n })),
    record: { start: week(0), count: 41 },
    week_start: start,
    has_meetings: count > 0,
  };
}
