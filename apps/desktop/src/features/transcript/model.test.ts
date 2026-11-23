import { describe, expect, it } from "vitest";
import type { Person, RecordingState, Segment, Speaker, Transcript } from "@/ipc/types";
import {
  findMatches,
  groupTurns,
  hasUnknownSpeaker,
  matchPeople,
  nextPersonLabel,
  segmentAt,
  segmentChanges,
  selectedSpeakers,
  selectedText,
  speakerLook,
  speakerRoster,
  speakersLive,
  TURN_GAP_MS,
  upsertSegment,
  wordAtEdge,
  wordSpans,
} from "./model";

const seg = (
  id: string,
  speaker: string | null,
  start: number,
  text = id,
  final = true,
): Segment => ({
  id,
  note_id: "n",
  speaker_id: speaker,
  channel: speaker === "you" ? "mic" : "system",
  start_ms: start,
  end_ms: start + 2000,
  text,
  is_final: final,
});

const empty = (): Transcript => ({
  note_id: "n",
  speakers: [],
  segments: [],
  duration_ms: null,
  has_audio: false,
  speaker_detection: null,
  timed: true,
  quotes: [],
});

describe("upsertSegment", () => {
  it("inserts in time order and refines partials in place", () => {
    let t = empty();
    t = upsertSegment(t, seg("b", "you", 5000, "later"));
    t = upsertSegment(t, seg("a", "you", 1000, "Hel", false));
    expect(t.segments.map((s) => s.id)).toEqual(["a", "b"]);
    t = upsertSegment(t, seg("a", "you", 1000, "Hello there", false));
    t = upsertSegment(t, seg("a", "you", 1000, "Hello there.", true));
    expect(t.segments).toHaveLength(2);
    expect(t.segments[0]).toMatchObject({ text: "Hello there.", is_final: true });
  });

  it("returns a new object and keeps the old one intact", () => {
    const before = empty();
    const after = upsertSegment(before, seg("a", null, 0));
    expect(before.segments).toHaveLength(0);
    expect(after).not.toBe(before);
  });

  it("notices speakers it does not know yet", () => {
    expect(hasUnknownSpeaker(empty(), seg("a", "spk_9", 0))).toBe(true);
    expect(hasUnknownSpeaker(empty(), seg("a", null, 0))).toBe(false);
  });
});

describe("groupTurns", () => {
  it("merges consecutive lines by one speaker under one header", () => {
    const turns = groupTurns([
      seg("1", "you", 0),
      seg("2", "you", 3000),
      seg("3", "p1", 6000, "x", false),
      seg("4", "you", 9000),
    ]);
    expect(turns.map((t) => t.segments.map((s) => s.id))).toEqual([["1", "2"], ["3"], ["4"]]);
    expect(turns[1]?.partial).toBe(true);
    expect(turns[0]?.id).toBe("1");
  });

  it("splits a long pause and unattributed channels", () => {
    const turns = groupTurns([
      seg("1", "you", 0),
      seg("2", "you", 2000 + TURN_GAP_MS + 1),
      { ...seg("3", null, 300_000), channel: "mic" },
      { ...seg("4", null, 302_000), channel: "system" },
    ]);
    expect(turns).toHaveLength(4);
  });
});

describe("speakerRoster", () => {
  it("lists each voice once, in order of first words, with their share of the talking", () => {
    const turns = groupTurns([
      seg("1", "p1", 0),
      seg("2", "you", 3000),
      seg("3", "p1", 6000),
      seg("4", "p1", 9000),
    ]);
    const roster = speakerRoster(turns);
    expect(roster.map((r) => r.key)).toEqual(["p1", "you"]);
    expect(roster[0]).toMatchObject({ firstTurn: 0, share: 0.75 });
    expect(roster[1]).toMatchObject({ firstTurn: 1, share: 0.25 });
  });

  it("leaves lines with no speaker out, live or not", () => {
    const turns = groupTurns(
      [{ ...seg("1", null, 0), channel: "mic" }, seg("2", null, 3000), seg("3", "p1", 6000)],
      true,
    );
    expect(speakerRoster(turns).map((r) => r.key)).toEqual(["p1"]);
    expect(speakerRoster(groupTurns(turns.flatMap((t) => t.segments))).map((r) => r.key)).toEqual([
      "p1",
    ]);
  });
});

describe("speakerLook", () => {
  const people: Person[] = [
    {
      id: "p",
      name: "Sam Lee",
      color_index: 3,
      sample_count: 1,
      meeting_count: 1,
      created_at: "",
      is_self: false,
      last_met: null,
    },
  ];
  const speaker = (patch: Partial<Speaker>): Speaker => ({
    id: "s",
    note_id: "n",
    label: "Person 2",
    person_id: null,
    color_index: 5,
    is_self: false,
    ...patch,
  });

  it("lets a linked person's name and colour win", () => {
    expect(speakerLook(speaker({ person_id: "p" }), people)).toEqual({
      name: "Sam Lee",
      color: 3,
      unnamed: false,
    });
  });

  it("marks generic labels as unnamed, never the user", () => {
    expect(speakerLook(speaker({}), people).unnamed).toBe(true);
    expect(speakerLook(speaker({ label: "You", is_self: true }), people).unnamed).toBe(false);
  });

  it("shows no speaker as nobody, never You", () => {
    expect(speakerLook(undefined, people)).toMatchObject({ name: "", anonymous: true });
  });
});

describe("lookups", () => {
  it("finds the playing segment and text matches", () => {
    const segments = [seg("1", "you", 0, "Ship it"), seg("2", "p1", 5000, "ship the ship")];
    expect(segmentAt(segments, 5500)?.id).toBe("2");
    expect(segmentAt(segments, 3000)).toBeNull();
    const matches = findMatches(groupTurns(segments), "SHIP");
    expect(matches.map((m) => [m.segmentId, m.start])).toEqual([
      ["1", 0],
      ["2", 0],
      ["2", 9],
    ]);
  });
});

describe("speakers while recording", () => {
  const recording = (patch: Partial<RecordingState>): RecordingState => ({
    phase: "recording",
    note_id: "n1",
    meeting_app: null,
    started_at: null,
    auto_stop_at: null,
    live_speakers: false,
    paused_at: null,
    paused_ms: 0,
    meeting_live: null,
    ...patch,
  });

  it("shows lines without a speaker as pending only while speakers are told apart live", () => {
    expect(speakersLive(recording({ live_speakers: true }), "n1")).toBe(true);
    expect(speakersLive(recording({ live_speakers: true, phase: "stopping" }), "n1")).toBe(true);
    expect(speakersLive(recording({}), "n1")).toBe(false);
    expect(speakersLive(recording({ live_speakers: true, phase: "idle" }), "n1")).toBe(false);
    expect(speakersLive(recording({ live_speakers: true }), "another note")).toBe(false);
  });
});

describe("matchPeople", () => {
  const person = (id: string, name: string): Person => ({
    id,
    name,
    color_index: 1,
    sample_count: 0,
    meeting_count: 0,
    created_at: "2026-01-01T00:00:00Z",
    is_self: false,
    last_met: null,
  });
  const people = [person("me", "You"), person("w", "Will Worral"), person("l", "Lucile Chartier")];

  it("keeps everyone with no query, the current person first", () => {
    expect(matchPeople(people, "", "l").people.map((p) => p.id)).toEqual(["l", "me", "w"]);
    expect(matchPeople(people, "  ").exact).toBeUndefined();
  });

  it("puts word starts before other matches", () => {
    expect(matchPeople(people, "wor").people.map((p) => p.id)).toEqual(["w"]);
    const withMarc = [person("m", "Marc Bell"), ...people];
    expect(matchPeople(withMarc, "c").people.map((p) => p.id)).toEqual(["l", "m"]);
    expect(matchPeople(people, "char").people.map((p) => p.id)).toEqual(["l"]);
  });

  it("finds an exact name regardless of case", () => {
    const m = matchPeople(people, "will worral");
    expect(m.exact?.id).toBe("w");
    expect(m.people[0]?.id).toBe("w");
    expect(matchPeople(people, "Lucy").exact).toBeUndefined();
    expect(matchPeople(people, "Lucy").people).toEqual([]);
  });
});

describe("nextPersonLabel", () => {
  const speaker = (id: string, label: string): Speaker => ({
    id,
    note_id: "n",
    label,
    person_id: null,
    color_index: 1,
    is_self: false,
  });

  it("takes the lowest number nobody else uses", () => {
    const speakers = [speaker("a", "Person 1"), speaker("b", "Speaker 2"), speaker("c", "Sam Lee")];
    expect(nextPersonLabel(speakers, "c")).toBe("Person 3");
    expect(nextPersonLabel([speaker("c", "Sam Lee")], "c")).toBe("Person 1");
  });

  it("keeps a speaker's own Person number", () => {
    expect(nextPersonLabel([speaker("a", "Person 4")], "a")).toBe("Person 4");
  });
});

describe("pending rows", () => {
  it("puts every pending line under one row while speakers are told apart live", () => {
    const segments = [seg("a", "s1", 0), seg("b", null, 3000), seg("c", null, 6000)];
    expect(groupTurns(segments, true).map((t) => t.segments.map((s) => s.id))).toEqual([
      ["a"],
      ["b", "c"],
    ]);
    expect(groupTurns(segments).map((t) => t.segments.map((s) => s.id))).toEqual([
      ["a"],
      ["b", "c"],
    ]);
  });

  it("keeps pending lines together across channels and long pauses", () => {
    const mic = { ...seg("b", null, 3000), channel: "mic" as const };
    const segments = [seg("a", "s1", 0), mic, seg("c", null, 3000 + TURN_GAP_MS * 2)];
    expect(groupTurns(segments, true).map((t) => t.segments.map((s) => s.id))).toEqual([
      ["a"],
      ["b", "c"],
    ]);
  });

  it("keeps the pending row's key as its first line is placed", () => {
    const before = groupTurns(
      [seg("a", "s1", 0), seg("b", null, 3000), seg("c", null, 6000)],
      true,
    );
    const after = groupTurns([seg("a", "s1", 0), seg("b", "s1", 3000), seg("c", null, 6000)], true);
    expect(before.map((t) => t.id)).toEqual(["a", "pending:0"]);
    expect(after.map((t) => t.id)).toEqual(["a", "pending:0"]);
  });
});

describe("segment motion", () => {
  const known = (segments: Segment[]) => new Map(segments.map((s) => [s.id, s]));

  it("finds new lines and pending lines that now have a speaker", () => {
    const before = [seg("a", "s1", 0), seg("b", null, 3000)];
    const after = [seg("a", "s1", 0), seg("b", "s1", 3000), seg("c", null, 6000)];
    const { added, resolved } = segmentChanges(known(before), after, true);
    expect(added).toEqual(["c"]);
    expect(resolved.map((s) => [s.id, s.speaker_id])).toEqual([["b", null]]);
  });

  it("does not treat a speaker change as resolving when lines cannot be pending", () => {
    const before = [seg("a", null, 0)];
    const after = [seg("a", "s1", 0)];
    expect(segmentChanges(known(before), after, false).resolved).toEqual([]);
  });
});

describe("selected words", () => {
  const text = "So the  plan, is fine.";
  it("snaps selection edges to whole words", () => {
    expect(wordSpans(text).map((w) => text.slice(w.start, w.end))).toEqual([
      "So",
      "the",
      "plan,",
      "is",
      "fine.",
    ]);
    // Starting mid-word takes the whole word; starting in a gap takes the next one.
    expect(wordAtEdge(text, 4, "start")?.index).toBe(1);
    expect(wordAtEdge(text, 7, "start")?.index).toBe(2);
    // Ending mid-word takes the whole word; ending in a gap takes the one before.
    expect(wordAtEdge(text, 9, "end")?.index).toBe(2);
    expect(wordAtEdge(text, 7, "end")?.index).toBe(1);
    expect(wordAtEdge(text, 0, "end")).toBeNull();
    expect(wordAtEdge(text, text.length, "start")).toBeNull();
  });

  it("joins the words between two ends, across lines", () => {
    const lines = [seg("a", "maya", 0, "one two three"), seg("b", "tomas", 2000, "four five")];
    const words = (id: string) => wordSpans(lines.find((l) => l.id === id)?.text ?? "");
    const at = (id: string, i: number) => ({ segmentId: id, word: words(id)[i] as never });
    expect(selectedText(lines, at("a", 1), at("b", 0))).toBe("two three four");
    expect(selectedText(lines, at("a", 2), at("a", 1))).toBe("");
    expect(selectedSpeakers(lines, at("a", 1), at("b", 0))).toEqual(["maya", "tomas"]);
  });
});
