/**
 * Transcript logic without React: live segment upserts, grouping segments into speaker turns,
 * how a speaker is shown (a linked Saved person's name and colour win), and find matches.
 */
import type { Person, Segment, Speaker, Transcript } from "@/ipc/types";

/** Segments in reading order. */
export function bySegmentTime(a: Segment, b: Segment): number {
  return a.start_ms - b.start_ms || a.id.localeCompare(b.id);
}

/**
 * Apply one live `transcript_segment` event: replace the segment with the same id (a partial
 * being refined, or finalised) or insert it in time order. Returns a new transcript.
 */
export function upsertSegment(transcript: Transcript, segment: Segment): Transcript {
  const index = transcript.segments.findIndex((s) => s.id === segment.id);
  let segments: Segment[];
  if (index >= 0) {
    segments = transcript.segments.slice();
    segments[index] = segment;
    // A refined segment can move if its start changed; keep order without a full sort.
    const prev = segments[index - 1];
    const next = segments[index + 1];
    if ((prev && bySegmentTime(prev, segment) > 0) || (next && bySegmentTime(segment, next) > 0)) {
      segments.sort(bySegmentTime);
    }
  } else {
    segments = transcript.segments.slice();
    let at = segments.length;
    while (at > 0 && bySegmentTime(segments[at - 1] as Segment, segment) > 0) at -= 1;
    segments.splice(at, 0, segment);
  }
  return { ...transcript, segments };
}

/** True when the transcript does not know a segment's speaker yet (fetch speakers again). */
export function hasUnknownSpeaker(transcript: Transcript, segment: Segment): boolean {
  return (
    Boolean(segment.speaker_id) && !transcript.speakers.some((s) => s.id === segment.speaker_id)
  );
}

export interface Turn {
  /** The first segment's id; stable while the turn grows. */
  id: string;
  speakerId: string | null;
  channel: Segment["channel"];
  startMs: number;
  endMs: number;
  segments: Segment[];
  /** Some text is still partial. */
  partial: boolean;
}

/** A pause this long starts a new turn even when the same person keeps talking. */
export const TURN_GAP_MS = 90_000;

/** Group consecutive segments by one speaker under one header. */
export function groupTurns(segments: readonly Segment[]): Turn[] {
  const turns: Turn[] = [];
  for (const segment of segments) {
    const last = turns[turns.length - 1];
    const sameSpeaker =
      last &&
      last.speakerId === segment.speaker_id &&
      (segment.speaker_id !== null || last.channel === segment.channel);
    if (last && sameSpeaker && segment.start_ms - last.endMs < TURN_GAP_MS) {
      last.segments.push(segment);
      last.endMs = Math.max(last.endMs, segment.end_ms);
      last.partial ||= !segment.is_final;
    } else {
      turns.push({
        id: segment.id,
        speakerId: segment.speaker_id,
        channel: segment.channel,
        startMs: segment.start_ms,
        endMs: segment.end_ms,
        segments: [segment],
        partial: !segment.is_final,
      });
    }
  }
  return turns;
}

export interface SpeakerLook {
  name: string;
  color: number;
  /** Not yet named: show "Name this speaker". */
  unnamed: boolean;
}

const GENERIC = /^(speaker|person)\s+\d+$/i;

/** How a speaker appears: a linked person's name and colour win over the meeting's label. */
export function speakerLook(
  speaker: Speaker | undefined,
  people: readonly Person[],
  channel: Segment["channel"] = "system",
): SpeakerLook {
  if (!speaker) {
    return channel === "mic"
      ? { name: "You", color: 1, unnamed: false }
      : { name: "Others", color: 2, unnamed: false };
  }
  const person = speaker.person_id ? people.find((p) => p.id === speaker.person_id) : undefined;
  if (person) return { name: person.name, color: person.color_index, unnamed: false };
  return {
    name: speaker.label,
    color: speaker.color_index,
    unnamed: !speaker.is_self && GENERIC.test(speaker.label.trim()),
  };
}

/** The segment playing at `ms`, by binary search over time-ordered segments. */
export function segmentAt(segments: readonly Segment[], ms: number): Segment | null {
  let lo = 0;
  let hi = segments.length - 1;
  let found: Segment | null = null;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const s = segments[mid] as Segment;
    if (s.start_ms <= ms) {
      found = s;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return found && ms <= Math.max(found.end_ms, found.start_ms + 1000) ? found : null;
}

/** The turn index holding `ms` (or the first one after it), for search hits and seeking. */
export function turnIndexAt(turns: readonly Turn[], ms: number): number {
  if (turns.length === 0) return -1;
  for (let i = 0; i < turns.length; i++) {
    const t = turns[i] as Turn;
    if (ms <= t.endMs) return i;
  }
  return turns.length - 1;
}

export interface FindMatch {
  segmentId: string;
  turnIndex: number;
  start: number;
  end: number;
}

/** Every case-insensitive occurrence of `query` in the turns, in reading order. */
export function findMatches(turns: readonly Turn[], query: string): FindMatch[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const out: FindMatch[] = [];
  turns.forEach((turn, turnIndex) => {
    for (const segment of turn.segments) {
      const text = segment.text.toLowerCase();
      let at = text.indexOf(q);
      while (at >= 0) {
        out.push({ segmentId: segment.id, turnIndex, start: at, end: at + q.length });
        at = text.indexOf(q, at + q.length);
      }
    }
  });
  return out;
}
