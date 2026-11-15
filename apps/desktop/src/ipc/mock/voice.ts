/**
 * Mock voice details for Saved people: a made-up but stable voiceprint per person, shaped like
 * what `get_person_voice` returns (64 bins, samples on a map around the voiceprint).
 */
import type { Person, PersonMeeting, PersonVoice, VoiceSampleShape } from "@/ipc/types";
import { iso } from "./util";

const BINS = 64;

/** Deterministic random numbers from a string, so a person always sounds the same. */
function rng(seed: string): () => number {
  let h = 2166136261;
  for (let i = 0; i < seed.length; i++) h = Math.imul(h ^ seed.charCodeAt(i), 16777619);
  return () => {
    h = (Math.imul(h, 1664525) + 1013904223) >>> 0;
    return h / 4294967296;
  };
}

function gauss(r: () => number): number {
  return Math.sqrt(-2 * Math.log(r() + 1e-9)) * Math.cos(2 * Math.PI * r());
}

function scaled(v: number[]): number[] {
  const m = Math.max(...v.map(Math.abs)) || 1;
  return v.map((x) => Math.max(-1, Math.min(1, x / m)));
}

function cosine(a: number[], b: number[]): number {
  let d = 0;
  let x = 0;
  let y = 0;
  a.forEach((v, i) => {
    const w = b[i] ?? 0;
    d += v * w;
    x += v * v;
    y += w * w;
  });
  return x && y ? d / Math.sqrt(x * y) : 0;
}

/** Past meetings a mock person was in, beyond the seeded ones, so their page has a history. */
const HISTORY = [
  "Weekly product sync",
  "Roadmap review",
  "Design critique",
  "Customer call, Northwind",
  "Sprint planning",
  "Hiring debrief",
  "Pricing workshop",
  "Launch retro",
  "Support triage",
  "Quarterly review",
];

/** A meeting in a seeded transcript that a mock person speaks in. */
export interface MockMeeting {
  id: string;
  title: string;
  at: string;
  duration_ms: number | null;
  speech_ms: number;
  lines: number;
}

/** The id of a person's `i`th mock sample (newest first, before any were removed). */
export const mockSampleId = (personId: string, i: number) => `${personId}_sample_${i}`;

/**
 * A mock person's meetings, newest first: the seeded ones they speak in, then made-up older
 * ones up to their meeting count. Meetings their voice was learnt from are the ones "you"
 * named them in, spread through the list, as many as they have samples.
 */
export function mockPersonMeetings(
  person: Person,
  now: Date,
  real: readonly MockMeeting[],
  samples: number,
): PersonMeeting[] {
  const r = rng(`${person.id}:meetings`);
  const total = Math.max(real.length, person.meeting_count);
  const learnt = Math.min(samples, total);
  const list: PersonMeeting[] = [];
  let at = now.getTime() - 26 * 3600_000;
  for (let i = 0; i < total; i++) {
    const seeded = real[i];
    if (seeded) at = Math.min(at, new Date(seeded.at).getTime());
    else at -= Math.round((1.5 + r() * 4) * 24 * 3600_000);
    const duration = seeded?.duration_ms ?? Math.round((18 + r() * 40) * 60_000);
    // Evenly spread which meetings someone picked them in.
    const picked = Math.floor(((i + 1) * learnt) / total) > Math.floor((i * learnt) / total);
    list.push({
      note_id: seeded?.id ?? `mock_meeting_${person.id}_${i}`,
      title: seeded?.title ?? HISTORY[Math.floor(r() * HISTORY.length)] ?? "Meeting",
      at: seeded?.at ?? iso(at),
      duration_ms: duration,
      speech_ms: seeded?.speech_ms ?? Math.round(duration * (0.12 + r() * 0.3)),
      lines: seeded?.lines ?? Math.round(8 + r() * 60),
      named_by: picked ? "you" : "cilica",
    });
  }
  return list;
}

/**
 * The voice details for a mock person, as of `now`. `removed` holds the samples taken out one
 * by one; samples come from the `meetings` someone named them in, newest first.
 */
export function mockPersonVoice(
  person: Person,
  now: Date,
  removed: ReadonlySet<string> = new Set(),
  meetings: readonly PersonMeeting[] = [],
): PersonVoice {
  const r = rng(person.id);
  const picked = meetings.filter((m) => m.named_by === "you");
  const count = Math.min(person.sample_count + removed.size, 30);
  // Fewer samples: a less settled voice.
  const wobble = count > 0 ? 0.1 + 0.25 / Math.sqrt(count) : 0;
  const mean = scaled(Array.from({ length: BINS }, () => gauss(r)));
  const all: VoiceSampleShape[] = Array.from({ length: count }, (_, i) => {
    // Now and then a sample that sounds unlike the others, as from a wrong pick.
    const odd = count >= 5 && i === 2;
    const bins = mean.map((m) =>
      Math.max(-1, Math.min(1, odd ? m * 0.6 + gauss(r) * 0.4 : m + gauss(r) * wobble)),
    );
    const meeting = picked[i];
    const angle = r() * Math.PI * 2;
    const dist = Math.min(1, Math.abs(gauss(r)) * wobble * 0.9);
    return {
      id: mockSampleId(person.id, i),
      bins,
      similarity: Math.max(0, cosine(bins, mean)),
      x: Math.cos(angle) * dist,
      y: Math.sin(angle) * dist,
      speech_ms: meeting?.speech_ms ?? Math.round(40_000 + r() * 140_000),
      note_id: meeting?.note_id ?? null,
      note_title: meeting?.title ?? "Weekly sync",
      created_at: meeting?.at ?? iso(now.getTime() - i * 3 * 24 * 3600_000),
    };
  });
  const samples = all.filter((s) => !removed.has(s.id));
  const kept = samples.length;
  const spread = mean.map((m, i) => {
    if (kept < 2) return 0;
    const v = samples.reduce((a, s) => a + ((s.bins[i] ?? 0) - m) ** 2, 0) / kept;
    return Math.min(1, Math.sqrt(v));
  });
  const named = kept > 0 ? meetings.filter((m) => m.named_by === "cilica").length : 0;
  return {
    person_id: person.id,
    speech_ms: samples.reduce((a, s) => a + s.speech_ms, 0),
    consistency: kept >= 2 ? samples.reduce((a, s) => a + (s.similarity ?? 0), 0) / kept : null,
    named,
    corrected: named > 3 ? 1 : 0,
    meeting_count: person.meeting_count,
    last_heard: meetings[0]?.at ?? null,
    shape: kept > 0 ? { mean, spread, samples } : null,
  };
}
