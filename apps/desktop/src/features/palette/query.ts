/**
 * Reads what was typed in the command palette as a search: the words to find, plus the dates,
 * times, people, categories, tags, links, meeting details, import sources and Trash it mentions.
 *
 * Plain language ("meetings with Sam last week", "edited yesterday afternoon", "long zoom calls")
 * and a small syntax (`in:Work`, `with:Sam`, `#tag`, `before:2026-11-01`, `is:meeting`,
 * `"exact phrase"`, `-word`) both work. Every filter it reads becomes a removable chip.
 *
 * Pure: "now", the locale and the vocabulary (people, categories, tags, apps, titles) come in
 * through `SearchVocabulary`, so it is fully testable and the store gets absolute times.
 */
import type { DateField, NoteKind, NoteQuery, TimeOfDay } from "@/ipc/types";

// ───────── Public shapes ─────────

export interface Named {
  id: string;
  name: string;
}

export interface SearchVocabulary {
  now: Date;
  people: readonly (Named & { is_self?: boolean })[];
  categories: readonly Named[];
  /** Tags in use, without `#`. Any `#tag` is read as a tag; these only help suggestions. */
  tags: readonly string[];
  meetingApps: readonly string[];
  /** Import providers: `id` as stored ("apple_notes"), `name` as shown ("Apple Notes"). */
  importProviders: readonly Named[];
  /** Note titles, for "links to <title>". */
  noteTitles: readonly string[];
  /** Read 01/02 as 1 February (true) or January 2 (false). */
  dayFirst: boolean;
  locale?: string;
}

export type FilterKind =
  | "date"
  | "time"
  | "kind"
  | "person"
  | "category"
  | "tag"
  | "link"
  | "app"
  | "duration"
  | "recording"
  | "quotes"
  | "source"
  | "trash"
  | "phrase"
  | "exclude";

/** One thing the search understood, shown as a chip. `start`/`end` index the typed text. */
export interface SearchFilter {
  kind: FilterKind;
  label: string;
  start: number;
  end: number;
  /** Written with the syntax (`in:Work`, `"…"`, `#tag`) rather than read from plain words. */
  typed: boolean;
}

export interface ParsedSearch {
  query: NoteQuery;
  /** The words left to find. */
  text: string;
  filters: SearchFilter[];
  /** Asks for more than words and phrases (a date, a person, a tag…). */
  hasDetails: boolean;
  /** Some filter came from plain words, so "search the words instead" makes sense. */
  readFromWords: boolean;
}

// ───────── Vocabulary ─────────

const MINUTE = 60_000;

const WEEKDAYS = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];
const WEEKDAY_SHORT = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
const MONTHS = [
  "january",
  "february",
  "march",
  "april",
  "may",
  "june",
  "july",
  "august",
  "september",
  "october",
  "november",
  "december",
];
const MONTH_SHORT = [
  "jan",
  "feb",
  "mar",
  "apr",
  "may",
  "jun",
  "jul",
  "aug",
  "sep",
  "oct",
  "nov",
  "dec",
];

/** Parts of the day, in local minutes after midnight (end may pass midnight). */
const PARTS_OF_DAY: Record<string, { start: number; end: number }> = {
  morning: { start: 5 * 60, end: 12 * 60 },
  afternoon: { start: 12 * 60, end: 17 * 60 },
  evening: { start: 17 * 60, end: 22 * 60 },
  night: { start: 21 * 60, end: 28 * 60 },
};

const KIND_WORDS = new Set(["meeting", "meetings", "call", "calls", "recording", "recordings"]);
const LONG_WORDS = new Set(["long", "longer", "lengthy"]);
const SHORT_WORDS = new Set(["short", "shorter", "quick", "brief"]);
const LONG_MEETING_MS = 45 * MINUTE;
const SHORT_MEETING_MS = 15 * MINUTE;
const RECENT_DAYS = 14;

/** Verbs that say which date a following date is about. */
const FIELD_VERBS: Record<string, DateField> = {
  created: "created",
  made: "created",
  written: "created",
  wrote: "created",
  added: "created",
  edited: "edited",
  updated: "edited",
  changed: "edited",
  modified: "edited",
  met: "met",
  recorded: "met",
  held: "met",
  deleted: "deleted",
  trashed: "deleted",
  removed: "deleted",
};
const FIELD_LABEL: Record<DateField, string> = {
  any: "",
  created: "Created",
  edited: "Edited",
  met: "Met",
  deleted: "Deleted",
};

const SYNTAX_KEYS = new Set([
  "in",
  "with",
  "tag",
  "from",
  "app",
  "is",
  "has",
  "no",
  "before",
  "after",
  "since",
  "until",
  "on",
  "date",
  "created",
  "edited",
  "met",
  "deleted",
  "links",
  "link",
  "longer",
  "shorter",
]);

// ───────── Tokens ─────────

interface Token {
  raw: string;
  /** Lowercased, outer punctuation trimmed. Empty for syntax, quoted and link tokens. */
  word: string;
  start: number;
  end: number;
  /** `key:value` with a known key. */
  key: string | null;
  value: string;
  /** `"…"`, possibly after a `-` or a key. */
  quoted: boolean;
  negated: boolean;
  /** `[[Title]]`. */
  link: string | null;
}

const TOKEN = /\[\[[^\]]*\]?\]?|-?(?:[a-z]+:)?"[^"]*"?|\S+/gi;

export function tokenize(input: string): Token[] {
  const tokens: Token[] = [];
  for (const m of input.matchAll(TOKEN)) {
    const raw = m[0];
    const start = m.index ?? 0;
    const token: Token = {
      raw,
      word: "",
      start,
      end: start + raw.length,
      key: null,
      value: "",
      quoted: false,
      negated: false,
      link: null,
    };
    if (raw.startsWith("[[")) {
      token.link =
        raw
          .replace(/^\[\[|\]+$/g, "")
          .split("|")[0]
          ?.trim() ?? "";
      tokens.push(token);
      continue;
    }
    let rest = raw;
    if (rest.length > 1 && rest.startsWith("-")) {
      token.negated = true;
      rest = rest.slice(1);
    }
    const keyed = /^([a-z]+):(.*)$/i.exec(rest);
    if (keyed?.[1] && SYNTAX_KEYS.has(keyed[1].toLowerCase()) && keyed[2] !== undefined) {
      token.key = keyed[1].toLowerCase();
      rest = keyed[2];
    }
    if (rest.startsWith('"')) {
      token.quoted = true;
      token.value = rest.replace(/^"|"$/g, "");
    } else if (token.key) {
      token.value = rest;
    } else {
      token.value = rest;
      token.word = rest.toLowerCase().replace(/^[([{'‘“]+|[)\]}'’”,.;!?]+$/g, "");
    }
    tokens.push(token);
  }
  return tokens;
}

// ───────── Dates ─────────

interface Range {
  start: Date;
  end: Date;
  label: string;
  /** Exactly one calendar day. */
  day: boolean;
  /** Already narrowed to part of a day ("this morning"); a time would conflict. */
  timed?: boolean;
}

function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

function addDays(d: Date, n: number): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + n, d.getHours(), d.getMinutes());
}

function atMinutes(day: Date, minutes: number): Date {
  return new Date(day.getFullYear(), day.getMonth(), day.getDate(), 0, minutes);
}

/** Monday of the week `d` is in. */
function startOfWeek(d: Date): Date {
  const day = startOfDay(d);
  return addDays(day, -((day.getDay() + 6) % 7));
}

function dayRange(day: Date, label: string): Range {
  const start = startOfDay(day);
  return { start, end: addDays(start, 1), label, day: true };
}

function capitalise(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** A label as it reads inside a longer one: "Since last week", but "Since Mon 10 Nov". */
function inPhrase(label: string): string {
  return /^(today|yesterday|tomorrow|tonight|this|last|past|previous|recently|a |an |one |two |three |\d+ \w+ ago)/i.test(
    label,
  )
    ? label.charAt(0).toLowerCase() + label.slice(1)
    : label;
}

function formatDay(d: Date, now: Date, locale?: string): string {
  const options: Intl.DateTimeFormatOptions = { weekday: "short", day: "numeric", month: "short" };
  if (d.getFullYear() !== now.getFullYear()) options.year = "numeric";
  return new Intl.DateTimeFormat(locale, options).format(d);
}

function formatMonth(year: number, month: number, locale?: string): string {
  return new Intl.DateTimeFormat(locale, { month: "long", year: "numeric" }).format(
    new Date(year, month, 1),
  );
}

export function formatMinutes(minutes: number, locale?: string): string {
  const m = ((minutes % 1440) + 1440) % 1440;
  const date = new Date(2000, 0, 1, Math.floor(m / 60), m % 60);
  const twelve = new Intl.DateTimeFormat(locale, { hour: "numeric" }).resolvedOptions().hour12;
  if (!twelve) {
    return new Intl.DateTimeFormat(locale, {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    }).format(date);
  }
  return new Intl.DateTimeFormat(locale, {
    hour: "numeric",
    minute: m % 60 ? "2-digit" : undefined,
    hour12: true,
  })
    .format(date)
    .replace(/\s/g, "")
    .toLowerCase();
}

function monthIndex(word: string, allowShort: boolean): number {
  const full = MONTHS.indexOf(word);
  if (full >= 0) return full;
  if (!allowShort) return -1;
  const short = word.replace(/\.$/, "");
  return short === "sept" ? 8 : MONTH_SHORT.indexOf(short);
}

function weekdayIndex(word: string, allowShort: boolean): number {
  const full = WEEKDAYS.indexOf(word);
  if (full >= 0) return full;
  return allowShort ? WEEKDAY_SHORT.indexOf(word.replace(/\.$/, "")) : -1;
}

function ordinal(word: string): number | null {
  const m = /^(\d{1,2})(st|nd|rd|th)?$/.exec(word);
  if (!m?.[1]) return null;
  const n = Number(m[1]);
  return n >= 1 && n <= 31 ? n : null;
}

function yearOf(word: string | undefined): number | null {
  if (!word || !/^(19|20)\d{2}$/.test(word)) return null;
  return Number(word);
}

function count(word: string | undefined): number | null {
  if (!word) return null;
  if (word === "a" || word === "an" || word === "one") return 1;
  const words: Record<string, number> = { two: 2, three: 3, four: 4, five: 5, six: 6, ten: 10 };
  if (word in words) return words[word] ?? null;
  return /^\d{1,3}$/.test(word) ? Number(word) : null;
}

type Unit = "day" | "week" | "month" | "year";
function unitOf(word: string | undefined): Unit | null {
  if (!word) return null;
  const w = word.replace(/s$/, "");
  return w === "day" || w === "week" || w === "month" || w === "year" ? w : null;
}

/** The most recent day with this weekday, today included. */
function lastWeekday(now: Date, weekday: number): Date {
  const today = startOfDay(now);
  return addDays(today, -((today.getDay() - weekday + 7) % 7));
}

/** The most recent this month: this year if it has started, last year otherwise. */
function recentMonthYear(now: Date, month: number): number {
  return month <= now.getMonth() ? now.getFullYear() : now.getFullYear() - 1;
}

function monthRange(year: number, month: number, locale?: string): Range {
  return {
    start: new Date(year, month, 1),
    end: new Date(year, month + 1, 1),
    label: formatMonth(year, month, locale),
    day: false,
  };
}

/** A calendar date, or null when it does not exist (31 February). */
function makeDate(year: number, month: number, day: number): Date | null {
  const d = new Date(year, month, day);
  return d.getMonth() === month && d.getDate() === day ? d : null;
}

/** Day and month without a year: this year, or last year when that day is still to come. */
function recentDate(now: Date, month: number, day: number): Date | null {
  const thisYear = makeDate(now.getFullYear(), month, day);
  if (thisYear && thisYear <= now) return thisYear;
  return makeDate(now.getFullYear() - 1, month, day);
}

interface Parsed<T> {
  value: T;
  /** Tokens used. */
  used: number;
}

class DateReader {
  constructor(
    private readonly tokens: readonly Token[],
    private readonly voc: SearchVocabulary,
  ) {}

  private w(i: number): string | undefined {
    const t = this.tokens[i];
    return t && !t.key && !t.quoted && !t.link && !t.negated ? t.word : undefined;
  }

  private day(day: Date): string {
    return formatDay(day, this.voc.now, this.voc.locale);
  }

  /**
   * A date or range starting at token `i`. `loose` also accepts a bare year and short month and
   * weekday names (after "in", "since"… where they cannot be ordinary words).
   */
  read(i: number, loose: boolean): Parsed<Range> | null {
    const now = this.voc.now;
    const today = startOfDay(now);
    const w0 = this.w(i);
    if (w0 === undefined) return null;
    const w1 = this.w(i + 1);
    const w2 = this.w(i + 2);

    // Named days, with an optional part of the day
    const named: Record<string, number> = { today: 0, yesterday: -1, tomorrow: 1 };
    if (w0 in named) {
      const base = addDays(today, named[w0] ?? 0);
      const part = w1 ? PARTS_OF_DAY[w1] : undefined;
      if (w1 && part) {
        return { value: this.partOfDay(base, part, `${capitalise(w0)} ${w1}`), used: 2 };
      }
      return { value: dayRange(base, capitalise(w0)), used: 1 };
    }
    if (w0 === "tonight") {
      return {
        value: this.partOfDay(today, PARTS_OF_DAY.evening ?? { start: 0, end: 0 }, "Tonight"),
        used: 1,
      };
    }
    if (w0 === "this" && w1 && PARTS_OF_DAY[w1] && w1 !== "night") {
      return {
        value: this.partOfDay(today, PARTS_OF_DAY[w1] ?? { start: 0, end: 0 }, `This ${w1}`),
        used: 2,
      };
    }
    if (w0 === "last" && w1 === "night") {
      return {
        value: this.partOfDay(
          addDays(today, -1),
          PARTS_OF_DAY.night ?? { start: 0, end: 0 },
          "Last night",
        ),
        used: 2,
      };
    }
    if (w0 === "recent" || w0 === "recently" || w0 === "lately") {
      return {
        value: {
          start: addDays(today, -RECENT_DAYS + 1),
          end: addDays(today, 1),
          label: "Recently",
          day: false,
        },
        used: 1,
      };
    }

    // this / last / past / previous + unit, or + a number of units
    if (w0 === "this" || w0 === "last" || w0 === "past" || w0 === "previous") {
      const unit = unitOf(w1);
      if (unit && w1 && !w1.endsWith("s")) {
        return {
          value: this.calendarUnit(unit, w0 === "this" ? 0 : -1, `${capitalise(w0)} ${w1}`),
          used: 2,
        };
      }
      const n = count(w1);
      const unitN = unitOf(w2);
      if (n !== null && unitN && w0 !== "this") {
        return { value: this.lastN(n, unitN, `${capitalise(w0)} ${w1} ${w2}`), used: 3 };
      }
      const weekday = w1 ? weekdayIndex(w1, true) : -1;
      if (weekday >= 0) {
        let d = lastWeekday(now, weekday);
        if (w0 !== "this" && d.getTime() === today.getTime()) d = addDays(d, -7);
        return { value: dayRange(d, `${capitalise(w0)} ${WEEKDAYS[weekday]}`), used: 2 };
      }
      const month = w1 ? monthIndex(w1, true) : -1;
      if (month >= 0 && w0 !== "past") {
        const year =
          w0 === "this"
            ? now.getFullYear()
            : recentMonthYear(now, month) - (month === now.getMonth() ? 1 : 0);
        return { value: monthRange(year, month, this.voc.locale), used: 2 };
      }
    }

    // N units ago
    const n0 = count(w0);
    if (n0 !== null && unitOf(w1) && w2 === "ago") {
      const unit = unitOf(w1) as Unit;
      const label = `${w0} ${w1} ago`;
      if (unit === "day")
        return { value: dayRange(addDays(today, -n0), capitalise(label)), used: 3 };
      return { value: this.calendarUnit(unit, -n0, capitalise(label)), used: 3 };
    }

    // Weekday
    const weekday = weekdayIndex(w0, loose);
    if (weekday >= 0) {
      const d = lastWeekday(now, weekday);
      return { value: dayRange(d, this.day(d)), used: 1 };
    }

    // ISO and numeric dates
    const iso = /^(\d{4})-(\d{1,2})(?:-(\d{1,2}))?$/.exec(w0);
    if (iso?.[1] && iso[2]) {
      const year = Number(iso[1]);
      const month = Number(iso[2]) - 1;
      if (!iso[3])
        return month >= 0 && month < 12
          ? { value: monthRange(year, month, this.voc.locale), used: 1 }
          : null;
      const d = makeDate(year, month, Number(iso[3]));
      return d ? { value: dayRange(d, this.day(d)), used: 1 } : null;
    }
    const numeric = /^(\d{1,2})[/.](\d{1,2})(?:[/.](\d{2}|\d{4}))?$/.exec(w0);
    if (numeric?.[1] && numeric[2]) {
      const [a, b] = [Number(numeric[1]), Number(numeric[2])];
      const [day, month] = this.voc.dayFirst ? [a, b - 1] : [b, a - 1];
      const yearText = numeric[3];
      const d = yearText
        ? makeDate(yearText.length === 2 ? 2000 + Number(yearText) : Number(yearText), month, day)
        : recentDate(now, month, day);
      return d ? { value: dayRange(d, this.day(d)), used: 1 } : null;
    }

    // Day month [year]: "14 nov", "14th november 2025"
    const d0 = ordinal(w0);
    if (d0 !== null && w1) {
      const month = monthIndex(w1, true);
      if (month >= 0) {
        const year = yearOf(w2);
        const d = year !== null ? makeDate(year, month, d0) : recentDate(now, month, d0);
        return d ? { value: dayRange(d, this.day(d)), used: year !== null ? 3 : 2 } : null;
      }
    }

    // Month [day] [year]: "nov 14", "november 2025", "november"
    const month = monthIndex(w0, loose || ordinal(w1 ?? "") !== null || yearOf(w1) !== null);
    if (month >= 0) {
      const d1 = w1 ? ordinal(w1) : null;
      if (d1 !== null) {
        const year = yearOf(w2);
        const d = year !== null ? makeDate(year, month, d1) : recentDate(now, month, d1);
        return d ? { value: dayRange(d, this.day(d)), used: year !== null ? 3 : 2 } : null;
      }
      const year = yearOf(w1);
      if (year !== null) return { value: monthRange(year, month, this.voc.locale), used: 2 };
      // "may" on its own is nearly always the verb.
      if (w0 === "may" && !loose) return null;
      return { value: monthRange(recentMonthYear(now, month), month, this.voc.locale), used: 1 };
    }

    // A bare year, where it cannot be an ordinary number
    const year = loose ? yearOf(w0) : null;
    if (year !== null) {
      return {
        value: {
          start: new Date(year, 0, 1),
          end: new Date(year + 1, 0, 1),
          label: String(year),
          day: false,
        },
        used: 1,
      };
    }
    return null;
  }

  private partOfDay(day: Date, part: { start: number; end: number }, label: string): Range {
    return {
      start: atMinutes(day, part.start),
      end: atMinutes(day, part.end),
      label,
      day: false,
      timed: true,
    };
  }

  /** The calendar week, month or year `offset` units from the current one. */
  private calendarUnit(unit: Unit, offset: number, label: string): Range {
    const now = this.voc.now;
    switch (unit) {
      case "day": {
        return dayRange(addDays(now, offset), label);
      }
      case "week": {
        const start = addDays(startOfWeek(now), 7 * offset);
        return { start, end: addDays(start, 7), label, day: false };
      }
      case "month": {
        const start = new Date(now.getFullYear(), now.getMonth() + offset, 1);
        return {
          start,
          end: new Date(start.getFullYear(), start.getMonth() + 1, 1),
          label,
          day: false,
        };
      }
      case "year": {
        const start = new Date(now.getFullYear() + offset, 0, 1);
        return { start, end: new Date(start.getFullYear() + 1, 0, 1), label, day: false };
      }
    }
  }

  /** The last `n` units up to and including today. */
  private lastN(n: number, unit: Unit, label: string): Range {
    const today = startOfDay(this.voc.now);
    const end = addDays(today, 1);
    const start =
      unit === "day"
        ? addDays(end, -n)
        : unit === "week"
          ? addDays(end, -7 * n)
          : unit === "month"
            ? new Date(end.getFullYear(), end.getMonth() - n, end.getDate())
            : new Date(end.getFullYear() - n, end.getMonth(), end.getDate());
    return { start, end, label, day: false };
  }

  /** A time of day at token `i`. `bare` also accepts a plain hour ("at 3"). */
  readTime(i: number, bare: boolean): Parsed<{ minutes: number; explicit: boolean }> | null {
    const w0 = this.w(i);
    if (w0 === undefined) return null;
    if (w0 === "noon" || w0 === "midday")
      return { value: { minutes: 12 * 60, explicit: true }, used: 1 };
    if (w0 === "midnight") return { value: { minutes: 0, explicit: true }, used: 1 };
    const m = /^(\d{1,2})(?:[:.](\d{2}))?(am|pm|a\.m\.|p\.m\.|h)?$/.exec(w0);
    if (!m?.[1]) return null;
    let hour = Number(m[1]);
    const minute = m[2] ? Number(m[2]) : 0;
    let suffix = m[3]?.replace(/\./g, "");
    let used = 1;
    const next = this.w(i + 1);
    if (!suffix && (next === "am" || next === "pm" || next === "a.m." || next === "p.m.")) {
      suffix = next.replace(/\./g, "");
      used = 2;
    }
    if (hour > 23 || minute > 59) return null;
    if (!suffix && !m[2] && !bare) return null;
    if (suffix === "pm" && hour < 12) hour += 12;
    if (suffix === "am" && hour === 12) hour = 0;
    const explicit = Boolean(suffix && suffix !== "h") || hour > 12 || Boolean(m[2] && hour >= 10);
    // A bare afternoon hour ("at 3"): nobody means 3 in the morning.
    if (!explicit && hour >= 1 && hour <= 7) hour += 12;
    return { value: { minutes: hour * 60 + minute, explicit: explicit || suffix === "h" }, used };
  }
}

// ───────── Durations ─────────

function readDuration(words: readonly (string | undefined)[]): Parsed<number> | null {
  const [a, b] = words;
  if (!a) return null;
  // "30m", "1.5h", "90min"
  const packed = /^(\d+(?:\.\d+)?)(m|min|mins|minutes?|h|hr|hrs|hours?)$/.exec(a);
  if (packed?.[1] && packed[2]) return { value: toMs(Number(packed[1]), packed[2]), used: 1 };
  const n = a === "a" || a === "an" ? 1 : /^\d+(?:\.\d+)?$/.test(a) ? Number(a) : null;
  if (n === null || !b || !/^(m|min|mins|minutes?|h|hr|hrs|hours?)$/.test(b)) return null;
  return { value: toMs(n, b), used: 2 };
}

function toMs(n: number, unit: string): number {
  return Math.round(unit.startsWith("h") ? n * 60 * MINUTE : n * MINUTE);
}

function formatDuration(ms: number): string {
  const minutes = Math.round(ms / MINUTE);
  if (minutes % 60 === 0 && minutes >= 60) {
    const h = minutes / 60;
    return h === 1 ? "an hour" : `${h} hours`;
  }
  return `${minutes} min`;
}

// ───────── Names ─────────

/** The longest of `names` whose words start at token `i`; ties go to the first. */
function matchName<T>(
  tokens: readonly Token[],
  i: number,
  items: readonly T[],
  nameOf: (item: T) => readonly string[],
): Parsed<T> | null {
  let best: Parsed<T> | null = null;
  for (const item of items) {
    for (const name of nameOf(item)) {
      const words = name.toLowerCase().split(/\s+/).filter(Boolean);
      if (!words.length || (best && words.length <= best.used)) continue;
      const fits = words.every((word, k) => {
        const t = tokens[i + k];
        return t && !t.key && !t.quoted && !t.link && !t.negated && t.word === word;
      });
      if (fits) best = { value: item, used: words.length };
    }
  }
  return best;
}

/** A person's full name, and their first name when nobody else shares it. */
function personNames(people: SearchVocabulary["people"]) {
  const firsts = new Map<string, number>();
  for (const p of people) {
    const first = p.name.split(/\s+/)[0]?.toLowerCase() ?? "";
    firsts.set(first, (firsts.get(first) ?? 0) + 1);
  }
  return (p: SearchVocabulary["people"][number]): string[] => {
    const first = p.name.split(/\s+/)[0] ?? "";
    const names = [p.name];
    if (first !== p.name && firsts.get(first.toLowerCase()) === 1) names.push(first);
    if (p.is_self) names.push("me", "myself");
    return names;
  };
}

/** An app's full name and its last word ("Teams" for Microsoft Teams). */
function appNames(app: string): string[] {
  const words = app.split(/\s+/);
  const last = words.at(-1);
  return words.length > 1 && last && last.length > 3 ? [app, last] : [app];
}

function findPerson(people: SearchVocabulary["people"], text: string) {
  const t = text.toLowerCase().trim();
  return (
    people.find((p) => p.name.toLowerCase() === t) ??
    (t === "me" ? people.find((p) => p.is_self) : undefined) ??
    people.find((p) => p.name.toLowerCase().startsWith(t))
  );
}

function findNamed<T extends Named>(items: readonly T[], text: string): T | undefined {
  const t = text.toLowerCase().trim().replace(/_/g, " ");
  return (
    items.find(
      (c) => c.name.toLowerCase() === t || c.id.toLowerCase() === text.toLowerCase().trim(),
    ) ?? items.find((c) => c.name.toLowerCase().startsWith(t))
  );
}

// ───────── Parser ─────────

interface Pending {
  kind: number[];
  /** A date field from a verb, kept until its date arrives. */
  field: { field: DateField; start: number } | null;
}

/** Read `input` as a search. Empty input gives an empty query. */
export function parseSearch(input: string, voc: SearchVocabulary): ParsedSearch {
  const tokens = tokenize(input);
  const used = tokens.map(() => false);
  const dates = new DateReader(tokens, voc);
  const filters: SearchFilter[] = [];
  const q: Required<
    Pick<NoteQuery, "phrases" | "exclude" | "category_ids" | "person_ids" | "tags" | "links_to">
  > &
    NoteQuery = {
    phrases: [],
    exclude: [],
    category_ids: [],
    person_ids: [],
    tags: [],
    links_to: [],
  };
  let after: Date | null = null;
  let before: Date | null = null;
  let singleDay: Date | null = null;
  let timedRange = false;
  let time: (TimeOfDay & { label: string }) | null = null;
  let field: DateField = "any";
  let kind: NoteKind | null = null;
  let trash = false;
  const pending: Pending = { kind: [], field: null };
  const people = personNames(voc.people);
  const apps = [...new Set(voc.meetingApps)];

  const w = (i: number): string | undefined => {
    const t = tokens[i];
    return t && !used[i] && !t.key && !t.quoted && !t.link && !t.negated ? t.word : undefined;
  };
  const take = (from: number, n: number) => {
    for (let k = from; k < from + n; k++) used[k] = true;
  };
  const span = (from: number, n: number) => ({
    start: tokens[from]?.start ?? 0,
    end: tokens[from + n - 1]?.end ?? 0,
  });
  const add = (kindOf: FilterKind, label: string, from: number, n: number, typed = false) => {
    take(from, n);
    filters.push({ kind: kindOf, label, ...span(from, n), typed });
  };

  /** Narrow the date range; `mode` says which side of `r` it sets. */
  const applyRange = (r: Range, mode: "on" | "after" | "before" | "until") => {
    const start = mode === "on" || mode === "after" ? r.start : null;
    const end =
      mode === "on" ? r.end : mode === "before" ? r.start : mode === "until" ? r.end : null;
    if (start && (!after || start > after)) after = start;
    if (end && (!before || end < before)) before = end;
    singleDay = mode === "on" && r.day ? r.start : null;
    timedRange = mode === "on" && Boolean(r.timed);
  };

  /** Label a date filter, with the verb that came before it ("Edited yesterday"). */
  const dateFilter = (label: string, from: number, n: number, typed = false) => {
    let start = from;
    let text = label;
    if (pending.field) {
      start = pending.field.start;
      text = `${FIELD_LABEL[pending.field.field]} ${inPhrase(label)}`;
      field = pending.field.field;
      pending.field = null;
    }
    add("date", text, start, from - start + n, typed);
  };

  const prepositionMode = (word: string | undefined) => {
    switch (word) {
      case "since":
      case "after":
        return "after" as const;
      case "before":
        return "before" as const;
      case "until":
      case "till":
      case "through":
        return "until" as const;
      case "on":
      case "in":
      case "during":
      case "from":
      case "at":
        return "on" as const;
      default:
        return null;
    }
  };
  const PREFIX: Record<string, string> = { after: "Since", before: "Before", until: "Until" };

  /** A date, possibly after a preposition, at `i`. Reads only; applies nothing. */
  const peekDate = (i: number): boolean => {
    const mode = prepositionMode(w(i));
    return Boolean(
      (mode && dates.read(i + 1, true)) ||
        dates.read(i, false) ||
        dates.readTime(mode ? i + 1 : i, mode !== null),
    );
  };

  const setTime = (start: number, end: number, label: string, from: number, n: number) => {
    time = { start: ((start % 1440) + 1440) % 1440, end: ((end % 1440) + 1440) % 1440, label };
    let first = from;
    let text = label;
    if (pending.field) {
      first = pending.field.start;
      text = `${FIELD_LABEL[pending.field.field]} ${label.charAt(0).toLowerCase()}${label.slice(1)}`;
      field = pending.field.field;
      pending.field = null;
    }
    add("time", text, first, from - first + n);
  };

  // Syntax tokens first: they never depend on their neighbours.
  tokens.forEach((t, i) => {
    if (t.link !== null) {
      if (t.link) {
        q.links_to.push(t.link);
        add("link", `Links to ${t.link}`, i, 1, true);
      }
      return;
    }
    if (!t.key) {
      if (t.quoted && t.value.trim()) {
        if (t.negated) {
          q.exclude.push(t.value.trim());
          add("exclude", `Without “${t.value.trim()}”`, i, 1, true);
        } else {
          q.phrases.push(t.value.trim());
          add("phrase", `“${t.value.trim()}”`, i, 1, true);
        }
      } else if (t.negated && t.word && !t.quoted) {
        const word = t.value.replace(/[,.;!?]+$/, "");
        q.exclude.push(word);
        add("exclude", `Without “${word}”`, i, 1, true);
      } else if (/^#[\p{L}\p{N}_/-]*\p{L}[\p{L}\p{N}_/-]*$/u.test(t.value)) {
        const tag = t.value.slice(1);
        q.tags.push(tag);
        add("tag", `#${tag}`, i, 1, true);
      }
      return;
    }
    const value = t.value.trim();
    if (!value) return;
    readSyntax(t.key, value, i);
  });

  function readSyntax(key: string, value: string, i: number) {
    const lower = value.toLowerCase();
    switch (key) {
      case "in": {
        if (lower === "trash") {
          trash = true;
          return add("trash", "In Trash", i, 1, true);
        }
        if (lower === "none" || lower === "uncategorized" || lower === "uncategorised") {
          q.uncategorized = true;
          return add("category", "No category", i, 1, true);
        }
        const category = findNamed(voc.categories, value);
        if (category) {
          q.category_ids.push(category.id);
          return add("category", `In ${category.name}`, i, 1, true);
        }
        return;
      }
      case "with": {
        const person = findPerson(voc.people, value);
        if (person) {
          q.person_ids.push(person.id);
          return add("person", `With ${person.is_self ? "you" : person.name}`, i, 1, true);
        }
        return;
      }
      case "tag": {
        const tag = value.replace(/^#/, "");
        q.tags.push(tag);
        return add("tag", `#${tag}`, i, 1, true);
      }
      case "from": {
        const provider = findNamed(voc.importProviders, value);
        if (provider) {
          q.import_provider = provider.id;
          return add("source", `From ${provider.name}`, i, 1, true);
        }
        q.meeting_app = value;
        return add("app", `On ${value}`, i, 1, true);
      }
      case "app": {
        const app = apps.find((a) => a.toLowerCase().includes(lower)) ?? value;
        q.meeting_app = lower.length >= 3 ? value : app;
        return add("app", `On ${app}`, i, 1, true);
      }
      case "is": {
        if (/^(meeting|meetings|call|calls)$/.test(lower)) {
          kind = "meeting";
          return add("kind", "Meetings", i, 1, true);
        }
        if (/^(note|notes|written|manual)$/.test(lower)) {
          kind = "manual";
          return add("kind", "Written notes", i, 1, true);
        }
        if (/^(deleted|trashed)$/.test(lower)) {
          trash = true;
          return add("trash", "In Trash", i, 1, true);
        }
        return;
      }
      case "has":
      case "no": {
        const yes = key === "has";
        if (/^(recording|audio)$/.test(lower)) {
          q.has_recording = yes;
          return add("recording", yes ? "Has recording" : "No recording", i, 1, true);
        }
        if (/^quotes?$/.test(lower)) {
          q.has_quotes = yes;
          return add("quotes", yes ? "Has quotes" : "No quotes", i, 1, true);
        }
        return;
      }
      case "longer":
      case "shorter": {
        const ms = readDuration(
          lower
            .match(/^(\d+(?:\.\d+)?)([a-z]*)$/)
            ?.slice(1)
            .filter(Boolean) ?? [],
        );
        const parsed =
          ms ?? (/^\d+$/.test(lower) ? { value: Number(lower) * MINUTE, used: 1 } : null);
        if (!parsed) return;
        if (key === "longer") q.min_duration_ms = parsed.value;
        else q.max_duration_ms = parsed.value;
        kind = "meeting";
        return add(
          "duration",
          `${capitalise(key)} than ${formatDuration(parsed.value)}`,
          i,
          1,
          true,
        );
      }
      case "links":
      case "link": {
        q.links_to.push(value);
        return add("link", `Links to ${value}`, i, 1, true);
      }
      default: {
        // Dates: before:, after:, since:, until:, on:, date:, created:, edited:, met:, deleted:
        const sub = new DateReader(tokenize(value), voc);
        const r = sub.read(0, true);
        if (!r) return;
        const mode =
          key === "before"
            ? "before"
            : key === "after" || key === "since"
              ? "after"
              : key === "until"
                ? "until"
                : "on";
        if (key in FIELD_VERBS) field = FIELD_VERBS[key] ?? "any";
        if (key === "deleted") trash = true;
        applyRange(r.value, mode);
        const prefix = key in FIELD_VERBS ? FIELD_LABEL[field] : PREFIX[mode];
        const label = prefix ? `${prefix} ${inPhrase(r.value.label)}` : r.value.label;
        return add("date", label, i, 1, true);
      }
    }
  }

  // Plain words, left to right.
  for (let i = 0; i < tokens.length; i++) {
    const w0 = w(i);
    if (w0 === undefined) continue;
    const w1 = w(i + 1);

    // "between X and Y": dates, then times
    if (w0 === "between") {
      const andAt = tokens.findIndex((t, k) => k > i + 1 && t.word === "and");
      const a = dates.read(i + 1, true);
      const b = andAt > 0 ? dates.read(andAt + 1, true) : null;
      if (a && b && i + 1 + a.used === andAt) {
        applyRange({ ...a.value, end: b.value.end }, "on");
        singleDay = null;
        dateFilter(`${a.value.label} to ${inPhrase(b.value.label)}`, i, andAt - i + 1 + b.used);
        i = andAt + b.used;
        continue;
      }
      const ta = dates.readTime(i + 1, true);
      const tb = andAt > 0 ? dates.readTime(andAt + 1, true) : null;
      if (ta && tb && i + 1 + ta.used === andAt) {
        const [start, end] = timePair(ta.value, tb.value);
        setTime(
          start,
          end,
          `${formatMinutes(start, voc.locale)} to ${formatMinutes(end, voc.locale)}`,
          i,
          andAt - i + 1 + tb.used,
        );
        i = andAt + tb.used;
        continue;
      }
    }

    // "2-4pm", "14:00-15:30"
    const dash = /^(\d{1,2}(?::\d{2})?)(am|pm)?[-–](\d{1,2}(?::\d{2})?)(am|pm)?$/.exec(w0);
    if (dash?.[1] && dash[3] && (dash[2] || dash[4] || dash[1].includes(":"))) {
      const reader = new DateReader(
        tokenize(`${dash[1]}${dash[2] ?? ""} ${dash[3]}${dash[4] ?? ""}`),
        voc,
      );
      const ta = reader.readTime(0, true);
      const tb = reader.readTime(1, true);
      if (ta && tb) {
        const [start, end] = timePair(ta.value, tb.value, dash[4] ?? dash[2]);
        setTime(
          start,
          end,
          `${formatMinutes(start, voc.locale)} to ${formatMinutes(end, voc.locale)}`,
          i,
          1,
        );
        continue;
      }
    }

    // Trash: "in trash", "in the trash", "deleted", "trashed"
    if (
      (w0 === "in" || w0 === "from") &&
      (w1 === "trash" || (w1 === "the" && w(i + 2) === "trash"))
    ) {
      trash = true;
      add("trash", "In Trash", i, w1 === "trash" ? 2 : 3);
      continue;
    }

    // Verbs that pick the date field: only when a date follows
    const verb = FIELD_VERBS[w0];
    if (verb && peekDate(i + 1)) {
      pending.field = { field: verb, start: i };
      if (verb === "deleted") trash = true;
      if (verb === "met") kind = kind ?? "meeting";
      take(i, 1);
      continue;
    }
    if ((w0 === "deleted" || w0 === "trashed") && !trash) {
      trash = true;
      add("trash", "In Trash", i, 1);
      continue;
    }

    // Recording and quotes: "with recording", "without audio", "has quotes", "no quotes"
    if (w1 && /^(with|has|having|without|no)$/.test(w0)) {
      const yes = w0 !== "without" && w0 !== "no";
      const skip = w1 === "a" || w1 === "the" ? 1 : 0;
      const what = w(i + 1 + skip);
      if (what && /^(recording|recordings|audio)$/.test(what)) {
        q.has_recording = yes;
        add("recording", yes ? "Has recording" : "No recording", i, 2 + skip);
        continue;
      }
      if (what && /^quotes?$/.test(what)) {
        q.has_quotes = yes;
        add("quotes", yes ? "Has quotes" : "No quotes", i, 2 + skip);
        continue;
      }
    }

    // People: "with Sam", "with Sam Lee and Alex"
    if (w0 === "with" || w0 === "w/") {
      let k = i + 1;
      const found: Named[] = [];
      for (;;) {
        const person = matchName(tokens, k, voc.people, people);
        if (!person || used.slice(k, k + person.used).some(Boolean)) break;
        found.push(person.value);
        k += person.used;
        if (w(k) === "and" && matchName(tokens, k + 1, voc.people, people)) k += 1;
        else break;
      }
      if (found.length) {
        for (const p of found) q.person_ids.push(p.id);
        const names = found.map((p) =>
          voc.people.find((x) => x.id === p.id)?.is_self ? "you" : p.name,
        );
        add("person", `With ${names.join(" and ")}`, i, k - i);
        i = k - 1;
        continue;
      }
    }

    // Links: "links to Roadmap", "mentions Roadmap"
    if (/^(links|linking|linked|mentions|mentioning|references)$/.test(w0)) {
      const skip = w1 === "to" ? 1 : 0;
      const title = matchName(tokens, i + 1 + skip, voc.noteTitles, (t) => [t]);
      if (title) {
        q.links_to.push(title.value);
        add("link", `Links to ${title.value}`, i, 1 + skip + title.used);
        i += skip + title.used;
        continue;
      }
    }

    // Durations: "longer than 30 min", "over an hour", "under 10 minutes"
    {
      const longer = /^(longer|more|over|above)$/.test(w0) || (w0 === "at" && w1 === "least");
      const shorter = /^(shorter|less|under|below)$/.test(w0) || (w0 === "at" && w1 === "most");
      if (longer || shorter) {
        const skip = w1 === "than" || w1 === "least" || w1 === "most" ? 1 : 0;
        const d = readDuration([w(i + 1 + skip), w(i + 2 + skip)]);
        if (d) {
          if (longer) q.min_duration_ms = d.value;
          else q.max_duration_ms = d.value;
          kind = "meeting";
          add(
            "duration",
            `${longer ? "Longer" : "Shorter"} than ${formatDuration(d.value)}`,
            i,
            1 + skip + d.used,
          );
          i += skip + d.used;
          continue;
        }
      }
    }

    // "long meetings", "quick calls"
    if ((LONG_WORDS.has(w0) || SHORT_WORDS.has(w0)) && w1 && KIND_WORDS.has(w1)) {
      if (LONG_WORDS.has(w0)) q.min_duration_ms = LONG_MEETING_MS;
      else q.max_duration_ms = SHORT_MEETING_MS;
      kind = "meeting";
      const long = LONG_WORDS.has(w0);
      add(
        "duration",
        long
          ? `Longer than ${formatDuration(LONG_MEETING_MS)}`
          : `Shorter than ${formatDuration(SHORT_MEETING_MS)}`,
        i,
        1,
      );
      continue;
    }

    // Prepositions: "in Work", "on Zoom", "from Notion", "since Monday", "at 3pm"
    const mode = prepositionMode(w0);
    if (w0 === "in") {
      const category = matchName(tokens, i + 1, voc.categories, (c) => [c.name]);
      if (category) {
        q.category_ids.push(category.value.id);
        add("category", `In ${category.value.name}`, i, 1 + category.used);
        i += category.used;
        continue;
      }
    }
    if (w0 === "from" || (w0 === "imported" && w1 === "from")) {
      const skip = w0 === "imported" ? 1 : 0;
      const provider = matchName(tokens, i + 1 + skip, voc.importProviders, (p) => [
        p.name,
        p.id.replace(/_/g, " "),
      ]);
      if (provider) {
        q.import_provider = provider.value.id;
        add("source", `From ${provider.value.name}`, i, 1 + skip + provider.used);
        i += skip + provider.used;
        continue;
      }
    }
    if (/^(on|in|via|over|using|from|through)$/.test(w0)) {
      const app = matchName(tokens, i + 1, apps, appNames);
      if (app) {
        q.meeting_app = app.value;
        kind = "meeting";
        add("app", `On ${app.value}`, i, 1 + app.used);
        i += app.used;
        continue;
      }
    }
    if (mode) {
      // "from X to Y"
      const r = dates.read(i + 1, true);
      if (r && w0 === "from") {
        const toAt = i + 1 + r.used;
        if (w(toAt) === "to" || w(toAt) === "until" || w(toAt) === "till") {
          const r2 = dates.read(toAt + 1, true);
          if (r2) {
            applyRange({ ...r.value, end: r2.value.end }, "on");
            singleDay = null;
            dateFilter(
              `${r.value.label} to ${inPhrase(r2.value.label)}`,
              i,
              toAt - i + 1 + r2.used,
            );
            i = toAt + r2.used;
            continue;
          }
        }
      }
      if (r) {
        applyRange(r.value, mode);
        const prefix = PREFIX[mode];
        dateFilter(prefix ? `${prefix} ${inPhrase(r.value.label)}` : r.value.label, i, 1 + r.used);
        i += r.used;
        continue;
      }
    }
    if (w0 === "at" || w0 === "around" || w0 === "about" || w0 === "after" || w0 === "before") {
      const t = dates.readTime(i + 1, true);
      if (t) {
        const m = t.value.minutes;
        const label = formatMinutes(m, voc.locale);
        if (w0 === "after") setTime(m, 24 * 60, `After ${label}`, i, 1 + t.used);
        else if (w0 === "before") setTime(0, m, `Before ${label}`, i, 1 + t.used);
        else setTime(m - 15, m + 60, `Around ${label}`, i, 1 + t.used);
        i += t.used;
        continue;
      }
    }

    // A bare date or time (unless it is someone's name: "June")
    const r = matchName(tokens, i, voc.people, people) ? null : dates.read(i, false);
    if (r) {
      applyRange(r.value, "on");
      dateFilter(r.value.label, i, r.used);
      i += r.used - 1;
      continue;
    }
    const t = dates.readTime(i, false);
    if (t) {
      const m = t.value.minutes;
      setTime(m - 15, m + 60, `Around ${formatMinutes(m, voc.locale)}`, i, t.used);
      i += t.used - 1;
      continue;
    }

    // "zoom calls", "teams meeting": an app right before or after a meeting word
    const app = matchName(tokens, i, apps, appNames);
    if (app) {
      const nextWord = w(i + app.used);
      const prevWord = tokens[i - 1]?.word;
      if ((nextWord && KIND_WORDS.has(nextWord)) || (prevWord && KIND_WORDS.has(prevWord))) {
        q.meeting_app = app.value;
        kind = "meeting";
        add("app", `On ${app.value}`, i, app.used);
        i += app.used - 1;
        continue;
      }
    }

    if (KIND_WORDS.has(w0)) pending.kind.push(i);
  }

  // A verb whose date never came stays a word.
  if (pending.field) used[pending.field.start] = false;

  // "meetings" filters only alongside other details; on its own it is a word to find.
  const others = filters.some((f) => !["phrase", "exclude"].includes(f.kind));
  if (pending.kind.length && (others || kind === "meeting")) {
    kind = "meeting";
    const [first] = pending.kind;
    if (first !== undefined && !filters.some((f) => f.kind === "kind")) {
      add("kind", "Meetings", first, 1);
    }
    for (const k of pending.kind) used[k] = true;
  }

  // Times: on one day they narrow it; otherwise they apply to every day.
  const window = time as (TimeOfDay & { label: string }) | null;
  if (window) {
    const day = singleDay as Date | null;
    if (day && !timedRange) {
      after = atMinutes(day, window.start);
      before = atMinutes(day, window.end > window.start ? window.end : window.end + 1440);
    } else {
      q.time_of_day = { start: window.start, end: window.end };
    }
  }

  // Assemble
  const text = tokens
    .filter((_, i) => !used[i])
    .map((t) => t.raw)
    .join(" ");
  const afterDate = after as Date | null;
  const beforeDate = before as Date | null;
  if (kind === "meeting" && field === "any" && (afterDate || beforeDate || window)) field = "met";
  if (trash && (afterDate || beforeDate) && field === "any") field = "deleted";
  const query: NoteQuery = {
    ...q,
    text,
    after: afterDate?.toISOString() ?? null,
    before: beforeDate?.toISOString() ?? null,
    date_field: field,
    utc_offset_minutes: -voc.now.getTimezoneOffset(),
    kind,
    trash: trash ? "only" : "exclude",
  };
  filters.sort((a, b) => a.start - b.start);
  const hasDetails = filters.some((f) => f.kind !== "phrase" && f.kind !== "exclude");
  return {
    query,
    text,
    filters,
    hasDetails,
    readFromWords: filters.some((f) => !f.typed),
  };
}

/** Two times of a range, the first taking the second's am/pm when it has none ("2 and 4pm"). */
function timePair(
  a: { minutes: number; explicit: boolean },
  b: { minutes: number; explicit: boolean },
  suffix?: string,
): [number, number] {
  let start = a.minutes;
  const end = b.minutes;
  if (!a.explicit && b.explicit && suffix !== "am" && start >= 12 * 60 && end < 12 * 60)
    start -= 12 * 60;
  if (!a.explicit && start > end && start >= 12 * 60) start -= 12 * 60;
  return [start, end];
}

/** Remove the text a filter came from. */
export function withoutFilter(input: string, filter: SearchFilter): string {
  return `${input.slice(0, filter.start)} ${input.slice(filter.end)}`.replace(/\s+/g, " ").trim();
}

/** Whether this locale writes numeric dates day first (14/11) rather than month first (11/14). */
export function localeIsDayFirst(locale?: string): boolean {
  try {
    const parts = new Intl.DateTimeFormat(locale, {
      day: "numeric",
      month: "numeric",
    }).formatToParts(new Date(2020, 10, 25));
    const day = parts.findIndex((p) => p.type === "day");
    const month = parts.findIndex((p) => p.type === "month");
    return day < month;
  } catch {
    return true;
  }
}
