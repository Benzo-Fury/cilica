import { describe, expect, it } from "vitest";
import type { Person, PersonMeeting, PersonVoice, VoiceSampleShape } from "@/ipc/types";
import {
  deleteCopy,
  forgetCopy,
  heardOn,
  isOdd,
  learningReason,
  likelyDuplicates,
  matchesName,
  meetingLine,
  metLine,
  removeSampleCopy,
  samplesByMeeting,
  sortPeople,
  speechLength,
  talkShare,
  voiceBody,
  voiceHeadline,
  voiceState,
} from "./model";

const person = (patch: Partial<Person>): Person => ({
  id: "p",
  name: "Sam",
  color_index: 3,
  sample_count: 0,
  meeting_count: 2,
  created_at: "",
  is_self: false,
  last_met: null,
  ...patch,
});

const sample = (similarity: number, note_id: string | null = null): VoiceSampleShape => ({
  id: `s${similarity}`,
  bins: [],
  similarity,
  x: 0,
  y: 0,
  speech_ms: 60_000,
  note_id,
  note_title: "Standup",
  created_at: "2026-11-14T10:00:00Z",
});

const voice = (
  similarities: number[],
  consistency: number | null,
  patch: Partial<PersonVoice> = {},
): PersonVoice => ({
  person_id: "p",
  speech_ms: similarities.length * 60_000,
  consistency,
  named: 0,
  corrected: 0,
  meeting_count: 2,
  last_heard: null,
  shape: similarities.length
    ? { mean: [], spread: [], samples: similarities.map((s) => sample(s)) }
    : null,
  ...patch,
});

const meeting = (patch: Partial<PersonMeeting>): PersonMeeting => ({
  note_id: "n",
  title: "Standup",
  at: "2026-11-14T10:00:00Z",
  duration_ms: 40 * 60_000,
  speech_ms: 10 * 60_000,
  lines: 18,
  named_by: "you",
  ...patch,
});

describe("the people list", () => {
  it("puts you first, then whoever you met last", () => {
    const sorted = sortPeople([
      person({ id: "a", name: "Ana" }),
      person({ id: "b", name: "Bo", last_met: "2026-11-01T10:00:00Z" }),
      person({ id: "you", name: "You", is_self: true }),
      person({ id: "c", name: "Cy", last_met: "2026-11-10T10:00:00Z" }),
    ]);
    expect(sorted.map((p) => p.id)).toEqual(["you", "c", "b", "a"]);
  });

  it("finds people by the start of any name, ignoring accents", () => {
    const lucile = person({ name: "Lucile Chârtier" });
    expect(matchesName(lucile, "char")).toBe(true);
    expect(matchesName(lucile, "luc")).toBe(true);
    expect(matchesName(lucile, "tier")).toBe(false);
    expect(matchesName(lucile, "  ")).toBe(true);
  });

  it("says when you last met someone", () => {
    const now = new Date(2026, 10, 14, 12);
    expect(metLine(person({ meeting_count: 0 }), now)).toBe("Not in a meeting yet");
    expect(
      metLine(person({ meeting_count: 9, last_met: new Date(2026, 10, 14, 9).toISOString() }), now),
    ).toBe("9 meetings · last met today");
  });

  it("spots someone saved twice", () => {
    const sam = person({ id: "1", name: "Sam" });
    const samLee = person({ id: "2", name: "Sam Lee" });
    const samOrtiz = person({ id: "3", name: "Sam Ortiz" });
    const you = person({ id: "4", name: "Sam Me", is_self: true });
    expect(likelyDuplicates(sam, [sam, samLee, samOrtiz, you]).map((p) => p.id)).toEqual([
      "2",
      "3",
    ]);
    expect(likelyDuplicates(samLee, [sam, samLee, samOrtiz])).toEqual([sam]);
    expect(likelyDuplicates(you, [sam, you])).toEqual([]);
  });
});

describe("how well Cilica knows a voice", () => {
  it("is unknown, learning or known", () => {
    expect(voiceState(person({}))).toBe("unknown");
    expect(voiceState(person({ sample_count: 2 }), voice([0.9, 0.9], 0.9))).toBe("learning");
    expect(voiceState(person({ sample_count: 3 }), voice([0.9, 0.9, 0.9], 0.92))).toBe("known");
  });

  it("explains itself", () => {
    const sam = person({ name: "Sam Lee", sample_count: 3 });
    expect(voiceHeadline(sam, "known")).toBe("Cilica recognises Sam's voice");
    expect(voiceHeadline(person({ is_self: true }), "unknown")).toBe(
      "Cilica doesn't know your voice yet",
    );
    expect(voiceBody(sam, "known", voice([0.9], 0.9, { named: 6, corrected: 1 }))).toBe(
      "It named them on its own in 6 meetings, and you corrected it once.",
    );
    expect(voiceBody(sam, "unknown")).toContain("pick Sam Lee for their speaker");
  });

  it("says why a voice is still being learnt", () => {
    const sam = person({ sample_count: 2 });
    expect(learningReason(sam, voice([0.9, 0.9], 0.9))).toContain("only 2 meetings");
    expect(learningReason(sam, voice([0.9, 0.9, 0.9], 0.92))).toBeNull();
    const shaky = learningReason(sam, voice([0.9, 0.85, 0.5], 0.75));
    expect(shaky).toContain("don't sound alike");
    expect(shaky).toContain("sounds unlike the others");
    expect(learningReason(sam, voice([], null))).toBeNull();
  });

  it("flags a meeting unlike the others only once there are enough to compare", () => {
    expect(isOdd(sample(0.5), 2)).toBe(false);
    expect(isOdd(sample(0.5), 3)).toBe(true);
    expect(isOdd(sample(0.9), 3)).toBe(false);
  });
});

describe("a person's meetings", () => {
  it("says how much they spoke", () => {
    expect(meetingLine(meeting({}))).toBe("Spoke 10 min of 40 min · 18 lines");
    expect(meetingLine(meeting({ duration_ms: null, lines: 1 }))).toBe("Spoke 10 min · 1 line");
    expect(meetingLine(meeting({ lines: 0 }))).toBe("Named, with no lines of their own");
    expect(talkShare(meeting({}))).toBe(0.25);
    expect(talkShare(meeting({ duration_ms: null }))).toBeNull();
  });

  it("finds the sample each meeting taught", () => {
    const v = voice([], null, {
      shape: { mean: [], spread: [], samples: [sample(0.9, "n1"), sample(0.8, null)] },
    });
    const map = samplesByMeeting(v);
    expect([...map.keys()]).toEqual(["n1"]);
    expect(samplesByMeeting(undefined).size).toBe(0);
  });

  it("rounds speech to minutes and hours", () => {
    expect(speechLength(20_000)).toBe("under 1 min");
    expect(speechLength(18 * 60_000)).toBe("18 min");
    expect(speechLength(130 * 60_000)).toBe("2 h 10 min");
  });

  it("names recent days", () => {
    const now = new Date(2026, 10, 14, 10);
    expect(heardOn(new Date(2026, 10, 14, 8).toISOString(), now)).toBe("Today");
    expect(heardOn(new Date(2026, 10, 13, 22).toISOString(), now)).toBe("Yesterday");
  });
});

describe("confirmations", () => {
  it("words them for you and for others", () => {
    expect(forgetCopy(person({ is_self: true, name: "You" })).title).toBe("Forget your voice?");
    expect(forgetCopy(person({ sample_count: 1 })).body).toContain("the 1 meeting");
    expect(deleteCopy(person({})).body).toContain("In 2 meetings");
    expect(removeSampleCopy(person({}), "Standup", true).body).toContain("only meeting");
    expect(removeSampleCopy(person({ is_self: true }), "Standup", false).title).toBe(
      "Stop learning your voice from “Standup”?",
    );
  });
});
