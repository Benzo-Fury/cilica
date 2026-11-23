/**
 * Transcript logic without React: live segment upserts, grouping segments into speaker turns,
 * how a speaker is shown (a linked Saved person's name and colour win), and find matches.
 */
import type { Person, RecordingState, Segment, Speaker, Transcript } from "@/ipc/types";

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

/**
 * Speakers are being told apart as this note's meeting is recorded: a line with no speaker yet
 * is "Pending" (Cilica has not decided who said it), not "You" or "Others".
 */
export function speakersLive(recording: RecordingState, noteId: string): boolean {
  return recording.note_id === noteId && recording.phase !== "idle" && recording.live_speakers;
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

/**
 * Group consecutive segments by one speaker under one header. With `pending` (speakers are told
 * apart live) every run of lines without a speaker yet sits under one "Pending" header, whatever
 * channel they came from and however long the pause, until Cilica places them. A pending row is
 * keyed by how many pending runs come before it, not by its first line, so it stays the same row
 * as its lines leave for their speakers.
 */
export function groupTurns(segments: readonly Segment[], pending = false): Turn[] {
  const turns: Turn[] = [];
  let pendingRuns = 0;
  for (const segment of segments) {
    const last = turns[turns.length - 1];
    const isPending = pending && segment.speaker_id === null;
    const sameSpeaker =
      last &&
      last.speakerId === segment.speaker_id &&
      (segment.speaker_id !== null || pending || last.channel === segment.channel);
    if (last && sameSpeaker && (isPending || segment.start_ms - last.endMs < TURN_GAP_MS)) {
      last.segments.push(segment);
      last.endMs = Math.max(last.endMs, segment.end_ms);
      last.partial ||= !segment.is_final;
    } else {
      turns.push({
        id: isPending ? `pending:${pendingRuns++}` : segment.id,
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

/** One voice heard in a meeting, for the speakers list. */
export interface RosterEntry {
  /** The speaker's id, or the channel ("You" / "Others") when speakers are not told apart. */
  key: string;
  speakerId: string | null;
  channel: Segment["channel"];
  /** The turn they first speak in. */
  firstTurn: number;
  /** Their part of the talking, 0 to 1. */
  share: number;
}

/**
 * Everyone heard, in the order they first spoke, with how much of the talking was theirs. Lines
 * with no speaker (not decided yet, or nobody could be told apart) are nobody.
 */
export function speakerRoster(turns: readonly Turn[]): RosterEntry[] {
  const byKey = new Map<string, RosterEntry & { spoken: number }>();
  let total = 0;
  turns.forEach((turn, index) => {
    if (!turn.speakerId) return;
    const key = turn.speakerId ?? turn.channel;
    let entry = byKey.get(key);
    if (!entry) {
      entry = {
        key,
        speakerId: turn.speakerId,
        channel: turn.channel,
        firstTurn: index,
        share: 0,
        spoken: 0,
      };
      byKey.set(key, entry);
    }
    for (const s of turn.segments) {
      const length = Math.max(0, s.end_ms - s.start_ms);
      entry.spoken += length;
      total += length;
    }
  });
  return [...byKey.values()].map(({ spoken, ...entry }) => ({
    ...entry,
    share: total > 0 ? spoken / total : 0,
  }));
}

export interface SpeakerLook {
  name: string;
  color: number;
  /** Not yet named: show "Name this speaker". */
  unnamed: boolean;
  /** Cilica has not decided who said it yet. */
  pending?: boolean;
  /** Nobody could be told apart for it: shown with no avatar or name. */
  anonymous?: boolean;
}

/** A line nobody could be told apart for. */
export const ANONYMOUS_LOOK: SpeakerLook = { name: "", color: 1, unnamed: false, anonymous: true };

/** A line whose speaker Cilica is still working out. */
export const PENDING_LOOK: SpeakerLook = {
  name: "Pending",
  color: 1,
  unnamed: false,
  pending: true,
};

const GENERIC = /^(speaker|person)\s+\d+$/i;
const GENERIC_NUMBER = /^(?:speaker|person)\s+(\d+)$/i;

/**
 * How a speaker appears: a linked person's name and colour win over the meeting's label. No
 * speaker is nobody Cilica could tell apart: no badge, never assumed to be You.
 */
export function speakerLook(speaker: Speaker | undefined, people: readonly Person[]): SpeakerLook {
  if (!speaker) return ANONYMOUS_LOOK;
  const person = speaker.person_id ? people.find((p) => p.id === speaker.person_id) : undefined;
  if (person) return { name: person.name, color: person.color_index, unnamed: false };
  return {
    name: speaker.label,
    color: speaker.color_index,
    unnamed: !speaker.is_self && GENERIC.test(speaker.label.trim()),
  };
}

/**
 * The name an unlinked speaker falls back to: "Person N" with the lowest N no other speaker in
 * the meeting uses ("Speaker 3" counts as taken too). A speaker already called that keeps it.
 */
export function nextPersonLabel(speakers: readonly Speaker[], speakerId: string): string {
  const own = speakers.find((s) => s.id === speakerId)?.label.trim() ?? "";
  if (/^person\s+\d+$/i.test(own)) return own;
  const taken = new Set(
    speakers
      .filter((s) => s.id !== speakerId)
      .map((s) => GENERIC_NUMBER.exec(s.label.trim())?.[1])
      .filter(Boolean)
      .map(Number),
  );
  let n = 1;
  while (taken.has(n)) n += 1;
  return `Person ${n}`;
}

/** Saved people in the speaker popup, narrowed by what was typed. */
export interface PeopleMatch {
  /** People whose name contains the query, best first (name starts with it, then the rest). */
  people: Person[];
  /** A person whose name is exactly the query (ignoring case), so "Add as new" is not offered. */
  exact: Person | undefined;
}

/**
 * Narrow Saved people by a typed name. An empty query keeps everyone in order, with
 * `firstId` (the person this speaker is now, or Cilica's guess) moved to the top.
 */
export function matchPeople(
  people: readonly Person[],
  query: string,
  firstId?: string | null,
): PeopleMatch {
  const q = query.trim().toLowerCase();
  const rank = (p: Person) => {
    const name = p.name.toLowerCase();
    if (!q) return p.id === firstId ? 0 : 1;
    if (name === q) return 0;
    if (name.startsWith(q) || name.split(/\s+/).some((w) => w.startsWith(q))) return 1;
    return name.includes(q) ? 2 : -1;
  };
  const ranked = people
    .map((person, index) => ({ person, index, r: rank(person) }))
    .filter((x) => x.r >= 0)
    .sort((a, b) => a.r - b.r || a.index - b.index)
    .map((x) => x.person);
  return { people: ranked, exact: q ? people.find((p) => p.name.toLowerCase() === q) : undefined };
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

/** What changed between two renders of a live transcript, for its entrance animations. */
export interface SegmentChanges {
  /** Segments never shown before: they rise into place. */
  added: string[];
  /** Pending segments whose speaker Cilica just worked out, as they were while pending. */
  resolved: Segment[];
}

/**
 * Compare the segments with the ones shown last time (`known`: id → the segment then). A segment
 * that had no speaker and now has one is resolved, but only while lines can be pending.
 */
export function segmentChanges(
  known: ReadonlyMap<string, Segment>,
  segments: readonly Segment[],
  pendingSpeakers: boolean,
): SegmentChanges {
  const added: string[] = [];
  const resolved: Segment[] = [];
  for (const s of segments) {
    const before = known.get(s.id);
    if (!before) added.push(s.id);
    else if (pendingSpeakers && !before.speaker_id && s.speaker_id) resolved.push(before);
  }
  return { added, resolved };
}

// ── Selected words ──

/** A word of a line: where it starts and ends in the line's text. */
export interface WordSpan {
  index: number;
  start: number;
  end: number;
}

/** The words of a line (split on whitespace, as the backend counts them). */
export function wordSpans(text: string): WordSpan[] {
  const out: WordSpan[] = [];
  for (const m of text.matchAll(/\S+/g)) {
    out.push({ index: out.length, start: m.index ?? 0, end: (m.index ?? 0) + m[0].length });
  }
  return out;
}

/**
 * The word a selection edge at character `offset` lands on. A start edge takes the word it is
 * in or the next one; an end edge (exclusive) the word it is in or the one before. `null` when
 * there is no such word (a start after the last word, an end before the first).
 */
export function wordAtEdge(text: string, offset: number, edge: "start" | "end"): WordSpan | null {
  const words = wordSpans(text);
  if (edge === "start") return words.find((w) => w.end > offset) ?? null;
  return words.findLast((w) => w.start < offset) ?? null;
}

/** One end of a word selection: a word of a line. */
export interface SelectedEnd {
  segmentId: string;
  word: WordSpan;
}

/**
 * The selected words between two ends (in transcript order, both included), joined by spaces.
 * Empty when the ends cross (the selection held no whole word).
 */
export function selectedText(
  segments: readonly Segment[],
  from: SelectedEnd,
  to: SelectedEnd,
): string {
  const first = segments.findIndex((s) => s.id === from.segmentId);
  const last = segments.findIndex((s) => s.id === to.segmentId);
  if (first < 0 || last < 0 || first > last) return "";
  if (first === last && from.word.index > to.word.index) return "";
  const parts: string[] = [];
  for (let i = first; i <= last; i++) {
    const words = wordSpans(segments[i]?.text ?? "");
    const a = i === first ? from.word.index : 0;
    const b = i === last ? to.word.index : words.length - 1;
    const text = segments[i]?.text ?? "";
    for (const w of words.slice(a, b + 1)) parts.push(text.slice(w.start, w.end));
  }
  return parts.join(" ");
}

/** The speakers of the lines a selection touches (null for lines with nobody). */
export function selectedSpeakers(
  segments: readonly Segment[],
  from: SelectedEnd,
  to: SelectedEnd,
): (string | null)[] {
  const first = segments.findIndex((s) => s.id === from.segmentId);
  const last = segments.findIndex((s) => s.id === to.segmentId);
  if (first < 0 || last < 0) return [];
  return [...new Set(segments.slice(first, last + 1).map((s) => s.speaker_id))];
}
