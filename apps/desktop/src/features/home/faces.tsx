/**
 * Home's faces, each drawn from the week's numbers. Home shows one at a time; Settings shows the
 * same faces small as a preview.
 */

import { useState } from "react";
import { ShortcutTip } from "@/app/keybinds";
import { openNote } from "@/app/router";
import type { HomeGlance, NoteLine, Pace, Silence, TalkShare, WritingHours } from "@/ipc/types";

type Data<K extends keyof HomeGlance> = NonNullable<HomeGlance[K]>;

import { useSettings } from "@/features/settings/hooks";
import { money, relativeDate } from "@/lib/format";
import {
  Avatar,
  BareNoteCard,
  Button,
  Caret,
  CoinStacks,
  CountUp,
  DecisionTimeline,
  DoodlePad,
  EmptyState,
  GlanceBars,
  HourDots,
  JarFigure,
  LineList,
  Misspelt,
  OvertimeTrack,
  PaceGauge,
  PullQuote,
  SilenceGap,
  SplitBar,
  TalkBar,
  UndoneCard,
} from "@/ui";
import {
  andList,
  clip,
  duration,
  firstName,
  type GlanceFace,
  hourLabel,
  jarFaceReady,
  minSec,
  minutes,
  numberWord,
  paceChange,
  possessive,
  quietStretch,
  savesPerDot,
  talkHeadline,
  timecode,
  weekday,
} from "./glance";

export const searchTip = (
  <ShortcutTip action="palette.open">Search every note and transcript</ShortcutTip>
);

/** One face, or nothing when the data has nothing for it. The doodle needs no data. */
export function FaceView({ face, glance }: { face: GlanceFace; glance?: HomeGlance }) {
  switch (face) {
    case "tics":
      return glance?.tics ? <TicsFace tics={glance.tics} /> : null;
    case "overheard":
      return glance?.quotes.length ? <OverheardFace quotes={glance.quotes} /> : null;
    case "talk":
      return glance?.talk ? <TalkFace talk={glance.talk} /> : null;
    case "pace":
      return glance?.pace ? <PaceFace pace={glance.pace} /> : null;
    case "silence":
      return glance?.silence ? <SilenceFace silence={glance.silence} /> : null;
    case "own_words":
      return glance?.own_lines.length ? <OwnWordsFace lines={glance.own_lines} /> : null;
    case "writing":
      return glance?.writing ? <WritingFace writing={glance.writing} /> : null;
    case "questions":
      return glance?.questions.length ? <QuestionsFace questions={glance.questions} /> : null;
    case "overtime":
      return glance?.overtime ? <OvertimeFace o={glance.overtime} /> : null;
    case "slow_decision":
      return glance?.slow_decision ? <SlowDecisionFace d={glance.slow_decision} /> : null;
    case "monologue":
      return glance?.monologue ? <MonologueFace m={glance.monologue} /> : null;
    case "bare_note":
      return glance?.bare_note ? <BareNoteFace note={glance.bare_note} /> : null;
    case "mid_sentence":
      return glance?.mid_sentence ? <MidSentenceFace count={glance.mid_sentence} /> : null;
    case "spelling":
      return glance?.spelling ? <SpellingFace fix={glance.spelling} /> : null;
    case "ghostwriter":
      return glance?.ghostwriter ? <GhostwriterFace g={glance.ghostwriter} /> : null;
    case "undos":
      return glance?.undos ? <UndosFace u={glance.undos} /> : null;
    case "swear_jar":
    case "whos_paying":
    case "clean_week": {
      const jar = glance?.swear_jar;
      if (!jar || !jarFaceReady(face, jar, { anyDay: true })) return null;
      if (face === "swear_jar") return <SwearJarFace jar={jar} />;
      return face === "whos_paying" ? <WhosPayingFace jar={jar} /> : <CleanWeekFace jar={jar} />;
    }
    case "doodle":
      return (
        <DoodlePad>
          <EmptyState art={false} title="Pick a note to open it" tip={searchTip}>
            Or doodle while you think. It fades.
          </EmptyState>
        </DoodlePad>
      );
  }
}

function TicsFace({ tics }: { tics: Data<"tics"> }) {
  const rows = [
    {
      id: "you",
      label: tics.you.phrase,
      value: tics.you.count,
      display: String(tics.you.count),
      color: "accent" as const,
      serif: true,
    },
    ...tics.others.map((o, i) => ({
      id: `${o.name}-${i}`,
      label: o.phrase,
      value: o.count,
      display: String(o.count),
      color: o.color_index ?? undefined,
      serif: true,
    })),
  ];
  const top = tics.others[0];
  return (
    <div className="nt-glance">
      <p className="nt-glance-line">
        You said <b className="nt-glance-hot">“{tics.you.phrase}”</b>{" "}
        <CountUp value={tics.you.count} /> times this week
      </p>
      <GlanceBars rows={rows} />
      {top?.name && (
        <span className="nt-glance-caption">
          “{top.phrase}” is {possessive(top.name)}. They use it like a comma.
        </span>
      )}
    </div>
  );
}

function OvertimeFace({ o }: { o: Data<"overtime"> }) {
  return (
    <div className="nt-glance">
      <p className="nt-glance-line">
        Your {o.title} usually takes <b>{minutes(o.usual_ms)} minutes</b>. This one took{" "}
        <b className="nt-glance-hot">{minutes(o.actual_ms)}</b>.{o.again ? " Again." : ""}
      </p>
      <OvertimeTrack
        usualMs={o.usual_ms}
        actualMs={o.actual_ms}
        usualLabel={`${minutes(o.usual_ms)} min, usual`}
        actualLabel={`${minutes(o.actual_ms)} min`}
      />
      {o.recent_ms.length > 1 && (
        <button type="button" className="nt-glance-link" onClick={() => openNote(o.note_id)}>
          The last {numberWord(o.recent_ms.length)} ran {andList(o.recent_ms.map(minutes))} minutes.
        </button>
      )}
    </div>
  );
}

function SlowDecisionFace({ d }: { d: Data<"slow_decision"> }) {
  const s = d.slowest;
  const n = s.decisions;
  const last = s.decided_at_ms[s.decided_at_ms.length - 1];
  const f = d.fastest;
  return (
    <div className="nt-glance">
      <p className="nt-glance-line">
        {possessive(weekday(s.held_at))} {s.title} took <b>{minutes(s.duration_ms)} minutes</b> to
        make <b className="nt-glance-hot">{n === 1 ? "1 decision" : `${n} decisions`}</b>
      </p>
      {s.decided_at_ms.length > 0 && last !== undefined && (
        <DecisionTimeline
          durationMs={s.duration_ms}
          marks={s.decided_at_ms}
          flag={`${n === 1 ? "decided here" : "the last one"}, ${minutes(last)} min in`}
          start="0 min"
          end={`${minutes(s.duration_ms)} min`}
        />
      )}
      {f ? (
        <button type="button" className="nt-glance-link" onClick={() => openNote(f.note_id)}>
          Your fastest was {possessive(weekday(f.held_at))} {f.title}: {f.decisions}{" "}
          {f.decisions === 1 ? "decision" : "decisions"} in {minutes(f.duration_ms)} minutes.
        </button>
      ) : (
        <button type="button" className="nt-glance-link" onClick={() => openNote(s.note_id)}>
          Open the notes
        </button>
      )}
    </div>
  );
}

function MonologueFace({ m }: { m: Data<"monologue"> }) {
  return (
    <div className="nt-glance">
      <p className="nt-glance-line">
        You talked for <b className="nt-glance-hot">{minSec(m.your_ms)}</b> without stopping
      </p>
      <GlanceBars
        rows={[
          {
            id: "you",
            label: "Your longest turn",
            value: m.your_ms,
            display: minSec(m.your_ms),
            color: "accent",
          },
          {
            id: "avg",
            label: "Everyone's average",
            value: m.average_ms,
            display: minSec(m.average_ms),
          },
        ]}
      />
      <button
        type="button"
        className="nt-glance-link"
        onClick={() => openNote(m.note_id, { panel: "transcript", atMs: m.at_ms })}
      >
        {m.note_title}.{" "}
        {m.record_name && m.record_ms
          ? `${m.record_name} still holds the record at ${minSec(m.record_ms)}.`
          : "Nobody else came close."}
      </button>
    </div>
  );
}

function BareNoteFace({ note }: { note: Data<"bare_note"> }) {
  const title = note.title.replace(/[.!?…]+$/, "");
  return (
    <div className="nt-glance">
      <BareNoteCard
        title={note.title}
        meta={`Created ${agoText(note.created_at)}`}
        onOpen={() => openNote(note.note_id)}
      />
      <p className="nt-glance-line">
        “{title}.” That's the whole <b className="nt-glance-hot">fucking</b> note.
      </p>
    </div>
  );
}

function MidSentenceFace({ count }: { count: number }) {
  return (
    <div className="nt-glance">
      <p className="nt-glance-line">
        <b className="nt-glance-hot">{count} notes</b> just stop in the middle of a sen
        <Caret />
      </p>
      <span className="nt-glance-caption">Like you were abducted.</span>
    </div>
  );
}

function SpellingFace({ fix }: { fix: Data<"spelling"> }) {
  return (
    <div className="nt-glance">
      <Misspelt word={fix.wrong} />
      <p className="nt-glance-line">
        Spellcheck fixed “{fix.word}” for you <b className="nt-glance-hot">{fix.count} times</b>
      </p>
      <span className="nt-glance-caption">Do you know how to spell it, or…?</span>
    </div>
  );
}

function GhostwriterFace({ g }: { g: Data<"ghostwriter"> }) {
  const you = 100 - g.cilica_pct;
  return (
    <div className="nt-glance">
      <SplitBar
        quiet={{ label: `You ${you}%`, pct: you }}
        loud={{ label: `Cilica ${g.cilica_pct}%`, pct: g.cilica_pct }}
      />
      <p className="nt-glance-line">
        Cilica wrote <b className="nt-glance-hot">{g.cilica_pct}%</b> of your notes
      </p>
      <span className="nt-glance-caption">{ghostwriterQuip(g.cilica_pct)}</span>
    </div>
  );
}

/** A dig that fits how much of the writing Cilica did. */
function ghostwriterQuip(pct: number): string {
  if (pct >= 90) return "At this point you're more of a prompt.";
  if (pct >= 75) return "You hold the pen. Cilica moves your hand.";
  if (pct >= 60) return "Bold of you to keep your name on these.";
  if (pct >= 40) return "You bring the ideas. Cilica brings the full stops.";
  if (pct >= 25) return "Cilica mostly fixes the bits you were sure about.";
  if (pct >= 10) return "You barely let Cilica help. It's taking it personally.";
  return "Writing it all yourself. Very artisanal. Very slow.";
}

function UndosFace({ u }: { u: Data<"undos"> }) {
  const latest = u.latest;
  return (
    <div className="nt-glance">
      {latest && (
        <UndoneCard summary={latest.summary} struck={latest.cilica_text} kept={latest.your_text} />
      )}
      <p className="nt-glance-line">
        You undid Cilica's edits <b className="nt-glance-hot">{u.count} times</b> this week
      </p>
      <span className="nt-glance-caption">
        That's {numberWord(u.count)} polite ways of saying piss off.
      </span>
    </div>
  );
}

/** How loud the jar's big line gets, by how full the jar is. */
function jarExclaim(count: number): string {
  if (count >= 40) return "Holy fucking shit.";
  if (count >= 15) return "Holy shit.";
  return "Language.";
}

function SwearJarFace({ jar }: { jar: Data<"swear_jar"> }) {
  const { data: settings } = useSettings();
  const price = settings?.swear_jar.price_cents ?? 0;
  return (
    <div className="nt-glance">
      <JarFigure count={jar.count} />
      <p className="nt-glance-line">
        <b className="nt-glance-hot">{jar.count} swears</b> this week. {jarExclaim(jar.count)}
      </p>
      <span className="nt-glance-caption">
        {price > 0 && `That's ${money(price * jar.count)} if this were real money. `}The jar empties
        Monday.
      </span>
    </div>
  );
}

function WhosPayingFace({ jar }: { jar: Data<"swear_jar"> }) {
  const you = jar.people.find((p) => p.name === null) ?? {
    name: null,
    color_index: null,
    count: 0,
  };
  const others = jar.people.filter((p) => p.name !== null);
  // The loudest other person, and someone who kept it clean if anyone did
  const clean = others.find((p) => p.count === 0);
  const shown = [others[0], clean && clean !== others[0] ? clean : others[1]].filter(
    (p): p is (typeof others)[number] => Boolean(p),
  );
  const top = jar.people[0];
  const loudest = top && top.name !== null && top.count > you.count ? firstName(top.name) : null;
  return (
    <div className="nt-glance">
      <CoinStacks
        columns={[
          { id: "you", label: "You", count: you.count },
          ...shown.map((p) => ({
            id: p.name ?? "",
            label: firstName(p.name ?? ""),
            count: p.count,
          })),
        ]}
      />
      {loudest ? (
        <p className="nt-glance-line">
          {loudest} owes the jar the most. <b className="nt-glance-hot">Fucking hell, {loudest}.</b>
        </p>
      ) : (
        <p className="nt-glance-line">
          You out-swore the whole team. <b className="nt-glance-hot">Fucking legend.</b>
        </p>
      )}
      <span className="nt-glance-caption">
        {clean
          ? `${firstName(clean.name ?? "")} hasn't sworn once. Someone check on ${firstName(clean.name ?? "")}.`
          : `${andList(jar.people.map((p) => `${p.name === null ? "You" : firstName(p.name)} ${p.count}`))}.`}
      </span>
    </div>
  );
}

function CleanWeekFace({ jar }: { jar: Data<"swear_jar"> }) {
  return (
    <div className="nt-glance">
      <JarFigure count={0} cobwebs />
      <p className="nt-glance-line">
        Not one swear this week. <b className="nt-glance-hot">Who the fuck are you?</b>
      </p>
      <span className="nt-glance-caption">
        Last week the jar took {jar.last_week}. It misses you.
      </span>
    </div>
  );
}

/** "41 days ago", "3 weeks ago": how long a note has sat there. */
function agoText(iso: string): string {
  const days = Math.max(1, Math.floor((Date.now() - Date.parse(iso)) / 86_400_000));
  return days === 1 ? "yesterday" : `${days} days ago`;
}

function OverheardFace({ quotes }: { quotes: HomeGlance["quotes"] }) {
  const [index, setIndex] = useState(() => Math.floor(Math.random() * quotes.length));
  const q = quotes[index % quotes.length];
  if (!q) return null;
  const at = timecode(q.at_ms);
  return (
    <div className="nt-glance">
      <PullQuote
        text={q.text}
        openLabel={`Play this moment in ${q.note_title}`}
        onOpen={() => openNote(q.note_id, { panel: "transcript", atMs: q.at_ms })}
        byline={
          <>
            {q.speaker && <Avatar name={q.speaker} color={q.color_index ?? 1} size="xs" />}
            <span>
              {q.speaker ? `${q.speaker} in ` : ""}
              {q.note_title}
            </span>
            <time>{at}</time>
          </>
        }
      />
      {quotes.length > 1 && (
        <Button variant="ghost" onClick={() => setIndex((i) => i + 1)}>
          Another one
        </Button>
      )}
    </div>
  );
}

function TalkFace({ talk }: { talk: TalkShare }) {
  const head = talkHeadline(talk);
  const slices = talk.slices.map((s, i) => {
    const pct = Math.round((s.ms / talk.total_ms) * 100);
    const who = s.is_self ? "You" : (s.name ?? "Everyone else");
    return {
      id: `${who}-${i}`,
      color: s.color_index,
      value: s.ms,
      detail: `${who}: ${duration(s.ms)}, ${pct}%`,
    };
  });
  const across = `${talk.meetings} ${talk.meetings === 1 ? "meeting" : "meetings"}`;
  return (
    <div className="nt-glance">
      <p className="nt-glance-line">
        {head.lead && `${head.lead} `}
        <b>{head.strong}</b> {head.tail}
      </p>
      <TalkBar
        slices={slices}
        caption={`${duration(talk.total_ms)} of conversation across ${across}`}
      />
    </div>
  );
}

function PaceFace({ pace }: { pace: Pace }) {
  const change = paceChange(pace);
  const parts = [change ? `Your pace this week, ${change}.` : "Your pace this week."];
  if (pace.others_wpm !== null) parts.push(`Everyone else: ${pace.others_wpm}.`);
  return (
    <div className="nt-glance">
      <PaceGauge value={pace.you_wpm} previous={pace.last_week_wpm} />
      <span className="nt-pace-number">
        <CountUp value={pace.you_wpm} />
        <small>words a minute</small>
      </span>
      <span className="nt-glance-caption">{parts.join(" ")}</span>
    </div>
  );
}

function SilenceFace({ silence }: { silence: Silence }) {
  const seconds = Math.round(silence.gap_ms / 1000);
  const asked = silence.before.trim().endsWith("?");
  const who = silence.is_self ? "you" : (silence.speaker ?? "someone");
  return (
    <div className="nt-glance">
      <span className="nt-glance-eyebrow">The longest silence this week</span>
      <SilenceGap label={`${seconds} s`} />
      <p className="nt-silence-after">
        after {who} {asked ? "asked" : "said"} <q>{clip(silence.before)}</q>
      </p>
      <button
        type="button"
        className="nt-glance-link"
        onClick={() => openNote(silence.note_id, { panel: "transcript", atMs: silence.at_ms })}
      >
        {silence.note_title} · {timecode(silence.at_ms)}
      </button>
    </div>
  );
}

/** "9 Oct" or "Yesterday": when a note was last changed, for a line's meta. */
function writtenWhen(iso: string): string {
  const when = relativeDate(iso);
  return /^\d+[mh]$|^Now$/.test(when) ? "today" : when;
}

function OwnWordsFace({ lines }: { lines: NoteLine[] }) {
  const [index, setIndex] = useState(() => Math.floor(Math.random() * lines.length));
  const line = lines[index % lines.length];
  if (!line) return null;
  return (
    <div className="nt-glance">
      <PullQuote
        text={line.text}
        openLabel={`Open ${line.note_title || "this note"}`}
        onOpen={() => openNote(line.note_id)}
        byline={
          <span>
            {line.said_by ?? "You"}, in {line.note_title || "an untitled note"}
          </span>
        }
      />
      {lines.length > 1 && (
        <Button variant="ghost" onClick={() => setIndex((i) => i + 1)}>
          Another one
        </Button>
      )}
    </div>
  );
}

function WritingFace({ writing }: { writing: WritingHours }) {
  const quiet = quietStretch(writing.hours);
  const perDot = savesPerDot(writing.hours);
  const peak = hourLabel(writing.peak_hour);
  const key = perDot === 1 ? "Each dot is one save." : `Each dot is up to ${perDot} saves.`;
  return (
    <div className="nt-glance">
      <HourDots hours={writing.hours} peak={writing.peak_hour} peakLabel={peak} perDot={perDot} />
      <p className="nt-glance-line" data-upright>
        You write most around {peak}.
      </p>
      <span className="nt-glance-caption">{quiet ? `${key} ${quiet}` : key}</span>
    </div>
  );
}

/** How many questions show at once. */
const QUESTIONS_SHOWN = 3;

function QuestionsFace({ questions }: { questions: NoteLine[] }) {
  const [page, setPage] = useState(0);
  const pages = Math.ceil(questions.length / QUESTIONS_SHOWN);
  const start = (page % pages) * QUESTIONS_SHOWN;
  const shown = questions.slice(start, start + QUESTIONS_SHOWN);
  return (
    <div className="nt-glance">
      <span className="nt-glance-eyebrow">Questions you asked yourself</span>
      <LineList
        key={start}
        items={shown.map((q, i) => ({
          id: `${q.note_id}-${start + i}`,
          text: q.text,
          meta: `${q.note_title || "Untitled"} · ${writtenWhen(q.written_at)}`,
          openLabel: `Open ${q.note_title || "this note"}`,
          onOpen: () => openNote(q.note_id),
        }))}
      />
      {pages > 1 && (
        <Button variant="ghost" onClick={() => setPage((p) => p + 1)}>
          More questions
        </Button>
      )}
    </div>
  );
}
