import { describe, expect, it } from "vitest";
import type { Person, RecordingState, Segment, Speaker, Transcript } from "@/ipc/types";
import {
  findMatches,
  groupTurns,
  hasUnknownSpeaker,
  segmentAt,
  speakerLook,
  speakersPending,
  TURN_GAP_MS,
  upsertSegment,
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
    expect(speakerLook(undefined, people, "mic").name).toBe("You");
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
    ...patch,
  });

  it("waits for the end of the meeting unless the service labels speakers live", () => {
    expect(speakersPending(recording({}), "n1")).toBe(true);
    expect(speakersPending(recording({ phase: "diarizing" }), "n1")).toBe(true);
    expect(speakersPending(recording({ live_speakers: true }), "n1")).toBe(false);
    expect(speakersPending(recording({ phase: "idle" }), "n1")).toBe(false);
    expect(speakersPending(recording({}), "another note")).toBe(false);
  });
});
