/** Home's facts from the last week of meetings: a rough copy of `crates/store/src/home.rs`. */
import type { HomeGlance, Overheard, Segment, Silence, TalkSlice } from "@/ipc/types";
import { decisions, firstNewLine, overtime, slowDecision, TicCounter, turns } from "./faces";
import type { MockContext } from "./state";
import { swearJar } from "./swears";
import { writtenGlance } from "./written";

const WEEK_MS = 7 * 24 * 3600_000;
/** The seed's segments run until the next one starts, so speech time is estimated from words. */
const MS_PER_WORD = 450;
/** Lower than the real minute so the small seed still shows a pace. */
const MIN_SPEECH_MS = 20_000;

const wordCount = (s: Segment) => s.text.split(/\s+/).filter(Boolean).length;
const spokenMs = (s: Segment) => Math.min(s.end_ms - s.start_ms, wordCount(s) * MS_PER_WORD + 600);
const wpm = (words: number, ms: number) =>
  ms >= MIN_SPEECH_MS ? Math.floor((words * 60_000) / ms) : null;
/** Lower than the real minute so the small seed still shows a monologue. */
const MIN_MONOLOGUE_MS = 20_000;
/** Undos needed before the count is a joke rather than a slip. */
const MIN_UNDOS = 3;

type Glance = HomeGlance;
type MeetingDecisions = NonNullable<Glance["slow_decision"]>["slowest"];

export function homeCommands({ state }: MockContext) {
  return {
    getHomeGlance(): HomeGlance {
      const since = Date.now() - WEEK_MS;
      const meetings = [...state.notes.values()]
        .filter((n) => !n.deleted_at && n.kind === "meeting")
        .map((n) => ({ note: n, held: n.started_at ?? n.created_at }))
        .filter(({ held }) => new Date(held).getTime() >= since)
        .sort((a, b) => b.held.localeCompare(a.held));
      const quotes: Overheard[] = [];
      const tics = new TicCounter();
      const decided: MeetingDecisions[] = [];
      let monologue: Glance["monologue"] = null;
      let record: { name: string; ms: number } | null = null;
      const talk = new Map<string, TalkSlice>();
      const you = { words: 0, ms: 0 };
      const others = { words: 0, ms: 0 };
      let talkMeetings = 0;
      let silence: Silence | null = null;
      for (const { note, held } of meetings) {
        const t = state.transcripts.get(note.id);
        if (!t || t.segments.length === 0) continue;
        const nameOf = (speakerId: string | null) => {
          const sp = t.speakers.find((s) => s.id === speakerId);
          if (!sp) return null;
          const person = state.people.find((p) => p.id === sp.person_id);
          return { name: person?.name ?? sp.label, color: sp.color_index };
        };
        const spoken = t.segments.filter((s) => s.is_final && s.text.trim());
        if (spoken.length) talkMeetings += 1;
        let reached = 0;
        const lines: { name: string; mine: boolean; start: number; end: number }[] = [];
        spoken.forEach((s, i) => {
          const sp = t.speakers.find((x) => x.id === s.speaker_id);
          const mine = sp ? sp.is_self : s.channel === "mic";
          const who = nameOf(s.speaker_id ?? null);
          const ms = spokenMs(s);
          const bucket = mine ? you : others;
          bucket.words += wordCount(s);
          bucket.ms += ms;
          const key = mine ? "\u0000you" : (who?.name ?? "Others");
          const slice = talk.get(key) ?? {
            name: mine ? "You" : (who?.name ?? "Others"),
            color_index: who?.color ?? 1,
            ms: 0,
            is_self: mine,
          };
          slice.ms += ms;
          talk.set(key, slice);
          tics.add(who?.name ?? "Others", mine, who?.color ?? 1, s.text);
          lines.push({
            name: slice.name ?? "Others",
            mine,
            start: s.start_ms,
            end: s.start_ms + ms,
          });
          const gap = s.start_ms - reached;
          if (i > 0 && gap >= 3000 && gap <= 60_000 && (!silence || gap > silence.gap_ms)) {
            const before = spoken[i - 1] as Segment;
            const bsp = t.speakers.find((x) => x.id === before.speaker_id);
            const bwho = nameOf(before.speaker_id ?? null);
            silence = {
              note_id: note.id,
              note_title: note.title,
              held_at: held,
              gap_ms: gap,
              at_ms: before.start_ms,
              before: before.text.trim(),
              speaker: bwho?.name ?? null,
              color_index: bwho?.color ?? null,
              is_self: bsp ? bsp.is_self : before.channel === "mic",
            };
          }
          reached = Math.max(reached, s.start_ms + ms);
        });
        // Turns: your longest against this meeting's average, and anyone else's longer one.
        const all = turns(lines);
        const average = all.reduce((a, x) => a + x.end - x.start, 0) / Math.max(1, all.length);
        for (const turn of all) {
          const len = turn.end - turn.start;
          if (turn.mine && len >= MIN_MONOLOGUE_MS && (!monologue || len > monologue.your_ms)) {
            monologue = {
              note_id: note.id,
              note_title: note.title,
              at_ms: turn.start,
              your_ms: len,
              average_ms: Math.round(average),
              record_name: null,
              record_ms: null,
            };
          } else if (!turn.mine && (!record || len > record.ms)) {
            record = { name: turn.name, ms: len };
          }
        }
        // Decisions, placed by the summary's timestamps.
        const d = decisions(note.body_md);
        const length = lengthOf(note);
        if (d.count > 0 && length !== null && length >= 10 * 60_000) {
          decided.push({
            note_id: note.id,
            title: note.title,
            held_at: held,
            duration_ms: length,
            decisions: d.count,
            decided_at_ms: d.at,
          });
        }
        for (const q of t.quotes ?? []) {
          const seg =
            t.segments.find((s) => s.start_ms <= q.at_ms && q.at_ms <= s.end_ms) ??
            [...t.segments].reverse().find((s) => s.start_ms <= q.at_ms);
          const who = seg ? nameOf(seg.speaker_id ?? null) : null;
          quotes.push({
            note_id: note.id,
            note_title: note.title,
            held_at: held,
            at_ms: q.at_ms,
            text: q.text,
            speaker: who?.name ?? null,
            color_index: who?.color ?? null,
          });
        }
      }
      const slices = [...talk.values()].sort((a, b) => b.ms - a.ms);
      const total = slices.reduce((a, x) => a + x.ms, 0);
      const youWpm = wpm(you.words, you.ms);
      const m = monologue as Glance["monologue"];
      const r = record as { name: string; ms: number } | null;
      if (m && r && r.ms > m.your_ms) {
        m.record_name = r.name;
        m.record_ms = r.ms;
      }
      // Overtime looks at every meeting, older ones included.
      const history = [
        ...state.pastMeetings.map((p) => ({
          note_id: p.id,
          title: p.title,
          held: Date.parse(p.held_at),
          ms: p.duration_ms,
        })),
        ...[...state.notes.values()]
          .filter((n) => !n.deleted_at && n.kind === "meeting")
          .flatMap((n) => {
            const ms = lengthOf(n);
            return ms === null
              ? []
              : [
                  {
                    note_id: n.id,
                    title: n.title,
                    held: Date.parse(n.started_at ?? n.created_at),
                    ms,
                  },
                ];
          }),
      ].sort((a, b) => a.held - b.held);
      return {
        tics: tics.result(),
        quotes: quotes.slice(0, 12),
        talk:
          slices.length > 1 && total >= MIN_SPEECH_MS
            ? { total_ms: total, meetings: talkMeetings, slices: foldSlices(slices) }
            : null,
        pace:
          youWpm === null
            ? null
            : { you_wpm: youWpm, others_wpm: wpm(others.words, others.ms), last_week_wpm: null },
        silence,
        ...writtenGlance(state),
        overtime: overtime(history, since),
        slow_decision: slowDecision(decided),
        monologue: m,
        undos: undosSince(state, since),
        swear_jar: state.settings.swear_jar.enabled
          ? swearJar(state, state.settings.swear_jar)
          : null,
      };
    },
  };
}

/** Four named slices and you; the rest fold into one nameless slice. */
function foldSlices(sorted: TalkSlice[]): TalkSlice[] {
  const out: TalkSlice[] = [];
  let rest = 0;
  sorted.forEach((x, i) => {
    if (i < 4 || x.is_self) out.push(x);
    else rest += x.ms;
  });
  if (rest) out.push({ name: null, color_index: null, ms: rest, is_self: false });
  return out;
}

/** A meeting's length: as recorded, else from its start and end times. */
function lengthOf(n: {
  duration_ms?: number | null;
  started_at: string | null;
  ended_at: string | null;
}): number | null {
  if (n.duration_ms) return n.duration_ms;
  if (n.started_at && n.ended_at) return Date.parse(n.ended_at) - Date.parse(n.started_at);
  return null;
}

/** Margin's actions undone this week, with the latest body edit's lines from each side. */
function undosSince(state: MockContext["state"], since: number): Glance["undos"] {
  const undone = state.aiActions
    .filter((a) => a.status === "undone" && Date.parse(a.created_at) >= since)
    .sort((a, b) => b.created_at.localeCompare(a.created_at));
  if (undone.length < MIN_UNDOS) return null;
  const latest = undone.find((a) => state.undoBodies.has(a.id)) ?? undone[0];
  const body = latest ? state.undoBodies.get(latest.id) : undefined;
  return {
    count: undone.length,
    latest: latest
      ? {
          note_id: body?.note_id ?? null,
          summary: latest.summary,
          margin_text: body ? firstNewLine(body.after_md, body.before_md) : null,
          your_text: body ? firstNewLine(body.before_md, body.after_md) : null,
        }
      : null,
  };
}
