/**
 * The swear jar, worked out the way `crates/store/src/swears.rs` does: swears said in meetings are
 * read from transcripts each time, swears typed into notes are kept as coins, and the jar empties
 * on Monday at local midnight (or by hand). The word list is a copy of `margin_domain::Swears`.
 */
import type { SwearJar, SwearJarSettings, SwearPerson, SwearWord } from "@/ipc/types";
import { SwearMatcher } from "@/lib/swears";
import type { MockContext, MockState } from "./state";
import { appError, iso, newId } from "./util";

// ---------- Word list ---------- //

const STRONG: string[][] = [
  [
    "fuck",
    "fucks",
    "fucked",
    "fucker",
    "fuckers",
    "fucking",
    "fuckin",
    "fuckup",
    "fuckups",
    "fuckwit",
    "fuckwits",
    "fuckhead",
    "fuckheads",
    "fuckface",
    "fuckery",
    "motherfucker",
    "motherfuckers",
    "motherfucking",
    "motherfuckin",
    "mofo",
    "clusterfuck",
    "wtf",
    "ffs",
    "stfu",
    "gtfo",
  ],
  [
    "shit",
    "shits",
    "shitty",
    "shitting",
    "shitted",
    "shite",
    "shitshow",
    "shithead",
    "shitheads",
    "shithole",
    "shitload",
    "shitstorm",
    "bullshit",
    "bullshitting",
    "horseshit",
    "dipshit",
    "apeshit",
    "batshit",
    "chickenshit",
  ],
  ["ass", "asshole", "assholes", "dumbass", "jackass", "smartass", "hardass"],
  ["arsehole", "arseholes"],
  ["bitch", "bitches", "bitching", "bitchy", "sonofabitch"],
  ["bastard", "bastards"],
  ["dick", "dicks", "dickhead", "dickheads"],
  ["prick", "pricks"],
  ["cunt", "cunts"],
  ["twat", "twats"],
  ["wanker", "wankers", "wank", "wanking"],
  ["bollocks", "bollock"],
  ["tosser", "tossers"],
  ["knobhead", "knobheads"],
  ["piss", "pissed", "pissing", "pisses", "pisstake", "pissy"],
  ["douche", "douchebag", "douchebags"],
];

const MILD: string[][] = [
  ["damn", "damned", "dammit", "damnit"],
  ["goddamn", "goddamned", "goddammit", "goddamnit"],
  ["bloody"],
  ["crap", "crappy", "crapped"],
  ["hell"],
  ["arse", "arses", "badass"],
  ["bugger", "buggered", "buggering"],
  ["sod", "sodding"],
  ["feck", "fecking"],
  ["frigging", "friggin"],
];

export const SWEAR_WORDS: SwearWord[] = [
  ...STRONG.flatMap((g) => g.map((word) => ({ word, root: g[0] as string, mild: false }))),
  ...MILD.flatMap((g) => g.map((word) => ({ word, root: g[0] as string, mild: true }))),
];

const matcher = new SwearMatcher(SWEAR_WORDS);

// ---------- Weeks ---------- //

const WEEKS = 9;
const WEEK_MS = 7 * 24 * 3600_000;
const WORD_LIMIT = 6;

/** Monday at local midnight of the week holding `now`. */
export function weekStart(now: number = Date.now()): number {
  const d = new Date(now);
  d.setHours(0, 0, 0, 0);
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
  return d.getTime();
}

// ---------- Counting ---------- //

/** The jar for the week starting `start`, counted the way `prefs` says. */
export function swearJar(state: MockState, prefs: SwearJarSettings, start = weekStart()): SwearJar {
  const from = Math.max(start, state.swearEmptied ? Date.parse(state.swearEmptied) : 0);
  const earliest = start - (WEEKS - 1) * WEEK_MS;
  const weeks = Array.from({ length: WEEKS }, () => 0);
  const people = new Map<string, SwearPerson>();
  const words = new Map<string, number>();
  let count = 0;
  let lastWeek = 0;
  let hasMeetings = false;

  const person = (key: string, p: Omit<SwearPerson, "count">) => {
    const found = people.get(key) ?? { ...p, count: 0 };
    people.set(key, found);
    return found;
  };
  const add = (at: number, root: string, key: string, p: Omit<SwearPerson, "count">) => {
    if (at < start) {
      const back = Math.floor((start - at) / WEEK_MS);
      if (WEEKS - 2 - back >= 0) weeks[WEEKS - 2 - back] = (weeks[WEEKS - 2 - back] ?? 0) + 1;
      if (back === 0) lastWeek += 1;
      return;
    }
    if (at < from) return;
    weeks[WEEKS - 1] = (weeks[WEEKS - 1] ?? 0) + 1;
    count += 1;
    person(key, p).count += 1;
    words.set(root, (words.get(root) ?? 0) + 1);
  };
  const you = { name: null, color_index: null };

  // Meetings: every final line, everyone in them
  if (prefs.transcripts) {
    for (const note of state.notes.values()) {
      if (note.deleted_at || note.kind !== "meeting") continue;
      const held = Date.parse(note.started_at ?? note.created_at);
      const t = state.transcripts.get(note.id);
      if (held < earliest || !t) continue;
      for (const s of t.segments) {
        if (!s.is_final || !s.text.trim()) continue;
        const sp = t.speakers.find((x) => x.id === s.speaker_id);
        const mine = sp ? sp.is_self : s.channel === "mic";
        const name =
          state.people.find((p) => p.id === sp?.person_id)?.name ?? sp?.label ?? "Others";
        const key = mine ? "you" : (sp?.person_id ?? sp?.id ?? note.id);
        const who = mine ? you : { name, color_index: sp?.color_index ?? 2 };
        if (held >= start) {
          hasMeetings = true;
          person(key, who);
        }
        for (const hit of matcher.find(s.text, true)) add(held + s.start_ms, hit.root, key, who);
      }
    }
  }

  // Notes: the coins you typed
  if (prefs.notes) {
    for (const coin of state.swearCoins) {
      const c = matcher.canonical(coin.word);
      if (!c) continue;
      add(Date.parse(coin.at), c.root, "you", you);
    }
  }

  const weekList = weeks.map((n, i) => ({
    start: iso(start - (WEEKS - 1 - i) * WEEK_MS),
    count: n,
  }));
  const record = weekList
    .filter((w) => w.count > 0)
    .sort((a, b) => b.count - a.count || b.start.localeCompare(a.start))[0];
  return {
    count,
    last_week: lastWeek,
    people: [...people.values()].sort(
      (a, b) => b.count - a.count || Number(a.name !== null) - Number(b.name !== null),
    ),
    words: [...words.entries()]
      .map(([word, n]) => ({ word, count: n }))
      .sort((a, b) => b.count - a.count || a.word.localeCompare(b.word))
      .slice(0, WORD_LIMIT),
    weeks: weekList,
    record: record ?? null,
    week_start: iso(start),
    has_meetings: hasMeetings,
  };
}

// ---------- Commands ---------- //

export function swearCommands({ state, emit }: MockContext) {
  const changed = () => emit({ type: "data_changed", scopes: ["swear_jar"], note_id: null });
  return {
    getSwearJar(): SwearJar {
      return swearJar(state, state.settings.swear_jar);
    },
    listSwearWords(): SwearWord[] {
      return SWEAR_WORDS;
    },
    addSwearCoin(noteId: string | null, word: string): null {
      const prefs = state.settings.swear_jar;
      if (!prefs.enabled || !prefs.notes) return null;
      const c = matcher.canonical(word);
      if (!c) throw appError("invalid", `“${word}” is not a word the swear jar counts.`);
      state.swearCoins.push({ id: newId("coin"), note_id: noteId, word: c.root, at: iso() });
      changed();
      return null;
    },
    emptySwearJar(): null {
      state.swearEmptied = iso();
      changed();
      return null;
    },
  };
}
