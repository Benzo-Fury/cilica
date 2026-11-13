import { describe, expect, it } from "vitest";
import { parseSearch, type SearchVocabulary, tokenize, withoutFilter } from "./query";
import { suggestFilters } from "./suggest";

// Saturday 14 November 2026, 10:00 local time.
const NOW = new Date(2026, 10, 14, 10, 0);

const voc: SearchVocabulary = {
  now: NOW,
  people: [
    { id: "p_me", name: "Neo", is_self: true },
    { id: "p_sam", name: "Sam Lee" },
    { id: "p_alex", name: "Alex Kim" },
    { id: "p_june", name: "June" },
  ],
  categories: [
    { id: "c_work", name: "Work" },
    { id: "c_ideas", name: "Product ideas" },
  ],
  tags: ["pricing"],
  meetingApps: ["Zoom", "Google Meet", "Microsoft Teams"],
  importProviders: [
    { id: "notion", name: "Notion" },
    { id: "apple_notes", name: "Apple Notes" },
  ],
  noteTitles: ["Roadmap", "Launch plan"],
  dayFirst: true,
  locale: "en-GB",
};

const local = (y: number, m: number, d: number, h = 0, min = 0) =>
  new Date(y, m, d, h, min).toISOString();

const parse = (input: string) => parseSearch(input, voc);
const labels = (input: string) => parse(input).filters.map((f) => f.label);

describe("words", () => {
  it("leaves plain words alone", () => {
    const p = parse("pricing page");
    expect(p.text).toBe("pricing page");
    expect(p.filters).toEqual([]);
    expect(p.hasDetails).toBe(false);
    expect(p.query.trash).toBe("exclude");
  });

  it("reads phrases and excluded words", () => {
    const p = parse('"launch plan" -draft -"old notes" budget');
    expect(p.query.phrases).toEqual(["launch plan"]);
    expect(p.query.exclude).toEqual(["draft", "old notes"]);
    expect(p.text).toBe("budget");
    expect(p.hasDetails).toBe(false);
    expect(p.readFromWords).toBe(false);
  });

  it("keeps meeting words as words when nothing else is asked", () => {
    const p = parse("meeting agenda");
    expect(p.text).toBe("meeting agenda");
    expect(p.query.kind).toBeNull();
  });

  it("does not read may as a month on its own", () => {
    expect(parse("we may ship").text).toBe("we may ship");
    expect(parse("in may").query.after).toBe(local(2026, 4, 1));
  });

  it("does not read a person's name as a month", () => {
    expect(parse("june").query.after).toBeNull();
  });

  it("splits tokens with quotes, keys and wiki links", () => {
    expect(tokenize('in:"Product ideas" [[Launch plan]] -x').map((t) => t.raw)).toEqual([
      'in:"Product ideas"',
      "[[Launch plan]]",
      "-x",
    ]);
  });
});

describe("dates", () => {
  it("reads named days", () => {
    const p = parse("yesterday");
    expect(p.query.after).toBe(local(2026, 10, 13));
    expect(p.query.before).toBe(local(2026, 10, 14));
    expect(p.text).toBe("");
    expect(labels("yesterday")).toEqual(["Yesterday"]);
    expect(parse("today").query.after).toBe(local(2026, 10, 14));
  });

  it("reads weeks, months and years", () => {
    // Monday 9 November is the start of this week.
    expect(parse("this week").query.after).toBe(local(2026, 10, 9));
    expect(parse("last week").query).toMatchObject({
      after: local(2026, 10, 2),
      before: local(2026, 10, 9),
    });
    expect(parse("last month").query).toMatchObject({
      after: local(2026, 9, 1),
      before: local(2026, 10, 1),
    });
    expect(parse("in 2025").query).toMatchObject({
      after: local(2025, 0, 1),
      before: local(2026, 0, 1),
    });
    expect(parse("last 7 days").query).toMatchObject({
      after: local(2026, 10, 8),
      before: local(2026, 10, 15),
    });
    expect(parse("3 days ago").query).toMatchObject({
      after: local(2026, 10, 11),
      before: local(2026, 10, 12),
    });
  });

  it("reads a bare year only after a preposition", () => {
    expect(parse("2025 roadmap").text).toBe("2025 roadmap");
  });

  it("reads weekdays as the most recent one", () => {
    // Today is Saturday: Tuesday is the 10th, "last saturday" is a week ago.
    expect(parse("tuesday").query.after).toBe(local(2026, 10, 10));
    expect(parse("on tue").query.after).toBe(local(2026, 10, 10));
    expect(parse("saturday").query.after).toBe(local(2026, 10, 14));
    expect(parse("last saturday").query.after).toBe(local(2026, 10, 7));
  });

  it("reads written dates", () => {
    expect(parse("14 nov").query.after).toBe(local(2026, 10, 14));
    expect(parse("nov 3rd").query.after).toBe(local(2026, 10, 3));
    expect(parse("3 december").query.after).toBe(local(2025, 11, 3));
    expect(parse("1 march 2024").query.after).toBe(local(2024, 2, 1));
    expect(parse("2026-11-01").query.after).toBe(local(2026, 10, 1));
    expect(parse("02/11").query.after).toBe(local(2026, 10, 2));
    expect(parseSearch("11/02", { ...voc, dayFirst: false }).query.after).toBe(local(2026, 10, 2));
    expect(parse("november").query).toMatchObject({
      after: local(2026, 10, 1),
      before: local(2026, 11, 1),
    });
    expect(parse("december").query.after).toBe(local(2025, 11, 1));
  });

  it("reads open ranges and spans", () => {
    expect(parse("since monday").query).toMatchObject({ after: local(2026, 10, 9), before: null });
    expect(parse("before 2026-11-01").query).toMatchObject({
      after: null,
      before: local(2026, 10, 1),
    });
    expect(parse("until yesterday").query.before).toBe(local(2026, 10, 14));
    expect(parse("between 1 nov and 5 nov").query).toMatchObject({
      after: local(2026, 10, 1),
      before: local(2026, 10, 6),
    });
    expect(parse("from 1 nov to 3 nov").query.before).toBe(local(2026, 10, 4));
    expect(labels("since last week")).toEqual(["Since last week"]);
  });

  it("reads which date a verb is about", () => {
    const p = parse("edited yesterday");
    expect(p.query.date_field).toBe("edited");
    expect(p.text).toBe("");
    expect(labels("edited yesterday")).toEqual(["Edited yesterday"]);
    expect(parse("created last week").query.date_field).toBe("created");
    // A verb without a date is a word.
    expect(parse("edited draft").text).toBe("edited draft");
  });

  it("reads deleted notes as Trash", () => {
    expect(parse("deleted").query.trash).toBe("only");
    const p = parse("deleted yesterday");
    expect(p.query).toMatchObject({ trash: "only", date_field: "deleted" });
    expect(parse("budget in the trash").query.trash).toBe("only");
    expect(parse("in:trash").query.trash).toBe("only");
  });

  it("reads recent", () => {
    expect(parse("recent meetings").query).toMatchObject({
      kind: "meeting",
      after: local(2026, 10, 1),
    });
  });
});

describe("times", () => {
  it("narrows a day to a time", () => {
    const p = parse("yesterday at 3pm");
    expect(p.query.after).toBe(local(2026, 10, 13, 14, 45));
    expect(p.query.before).toBe(local(2026, 10, 13, 16, 0));
    expect(p.query.time_of_day).toBeUndefined();
  });

  it("applies a time to every day without a date", () => {
    expect(parse("3pm").query.time_of_day).toEqual({ start: 14 * 60 + 45, end: 16 * 60 });
    expect(parse("between 2 and 4").query.time_of_day).toEqual({ start: 14 * 60, end: 16 * 60 });
    expect(parse("between 9 and 11").query.time_of_day).toEqual({ start: 9 * 60, end: 11 * 60 });
    expect(parse("2-4pm").query.time_of_day).toEqual({ start: 14 * 60, end: 16 * 60 });
    expect(parse("10-11am").query.time_of_day).toEqual({ start: 10 * 60, end: 11 * 60 });
    expect(parse("after 6pm").query.time_of_day).toEqual({ start: 18 * 60, end: 0 });
    expect(parse("at 15:30").query.time_of_day).toEqual({ start: 15 * 60 + 15, end: 16 * 60 + 30 });
  });

  it("reads parts of the day", () => {
    expect(parse("this morning").query).toMatchObject({
      after: local(2026, 10, 14, 5),
      before: local(2026, 10, 14, 12),
    });
    expect(parse("yesterday afternoon").query).toMatchObject({
      after: local(2026, 10, 13, 12),
      before: local(2026, 10, 13, 17),
    });
    expect(parse("last night").query.before).toBe(local(2026, 10, 14, 4));
    // On its own it is a word ("morning standup").
    expect(parse("morning standup").text).toBe("morning standup");
  });

  it("does not read plain numbers as times", () => {
    expect(parse("q3 plan 3").text).toBe("q3 plan 3");
  });

  it("labels times in the locale's clock", () => {
    expect(labels("around 3pm")).toEqual(["Around 15:00"]);
    expect(labels("edited at 3pm")).toEqual(["Edited around 15:00"]);
    const us = parseSearch("between 2 and 4:30pm", { ...voc, locale: "en-US", dayFirst: false });
    expect(us.filters.map((f) => f.label)).toEqual(["2pm to 4:30pm"]);
  });
});

describe("details", () => {
  it("reads people", () => {
    const p = parse("with sam lee");
    expect(p.query.person_ids).toEqual(["p_sam"]);
    expect(p.text).toBe("");
    expect(parse("with Alex").query.person_ids).toEqual(["p_alex"]);
    expect(parse("with sam and alex").query.person_ids).toEqual(["p_sam", "p_alex"]);
    expect(labels("with me")).toEqual(["With you"]);
    expect(parse("with:Sam").query.person_ids).toEqual(["p_sam"]);
    // Nobody called that: the words stay.
    expect(parse("with care").text).toBe("with care");
  });

  it("reads categories", () => {
    expect(parse("in work").query.category_ids).toEqual(["c_work"]);
    expect(parse("in product ideas").query.category_ids).toEqual(["c_ideas"]);
    expect(parse('in:"Product ideas"').query.category_ids).toEqual(["c_ideas"]);
    expect(parse("in:none").query.uncategorized).toBe(true);
  });

  it("reads tags and links", () => {
    expect(parse("#pricing #q3-plan").query.tags).toEqual(["pricing", "q3-plan"]);
    expect(parse("#123").query.tags).toEqual([]);
    expect(parse("links to roadmap").query.links_to).toEqual(["Roadmap"]);
    expect(parse("[[Launch plan]]").query.links_to).toEqual(["Launch plan"]);
    expect(parse("link:Roadmap").query.links_to).toEqual(["Roadmap"]);
  });

  it("reads meeting apps and kinds", () => {
    const p = parse("zoom calls");
    expect(p.query).toMatchObject({ meeting_app: "Zoom", kind: "meeting" });
    expect(p.text).toBe("");
    expect(parse("calls on teams").query.meeting_app).toBe("Microsoft Teams");
    expect(parse("meetings yesterday").query).toMatchObject({ kind: "meeting", date_field: "met" });
    expect(parse("is:note").query.kind).toBe("manual");
    // An app name alone is a word.
    expect(parse("zoom").text).toBe("zoom");
  });

  it("reads durations", () => {
    expect(parse("longer than 30 min").query.min_duration_ms).toBe(30 * 60_000);
    expect(parse("over an hour").query.min_duration_ms).toBe(60 * 60_000);
    expect(parse("under 10m").query.max_duration_ms).toBe(10 * 60_000);
    expect(parse("long meetings").query).toMatchObject({
      min_duration_ms: 45 * 60_000,
      kind: "meeting",
    });
    expect(parse("quick calls").query.max_duration_ms).toBe(15 * 60_000);
    expect(parse("longer:1h").query.min_duration_ms).toBe(60 * 60_000);
    expect(labels("longer than 90 min")).toEqual(["Longer than 90 min"]);
  });

  it("reads recordings, quotes and import sources", () => {
    expect(parse("with recording").query.has_recording).toBe(true);
    expect(parse("no audio").query.has_recording).toBe(false);
    expect(parse("has:quotes").query.has_quotes).toBe(true);
    expect(parse("from notion").query.import_provider).toBe("notion");
    expect(parse("imported from apple notes").query.import_provider).toBe("apple_notes");
  });

  it("puts it all together", () => {
    const p = parse("pricing meetings with Sam on zoom last week #launch");
    expect(p.text).toBe("pricing");
    expect(p.query).toMatchObject({
      kind: "meeting",
      person_ids: ["p_sam"],
      meeting_app: "Zoom",
      tags: ["launch"],
      after: local(2026, 10, 2),
      before: local(2026, 10, 9),
      date_field: "met",
    });
    expect(p.filters.map((f) => f.kind)).toEqual(["kind", "person", "app", "date", "tag"]);
    expect(p.readFromWords).toBe(true);
    expect(p.query.utc_offset_minutes).toBe(-NOW.getTimezoneOffset());
  });
});

describe("withoutFilter", () => {
  it("removes the words a filter came from", () => {
    const input = "pricing with Sam last week";
    const p = parse(input);
    const person = p.filters.find((f) => f.kind === "person");
    expect(person && withoutFilter(input, person)).toBe("pricing last week");
  });
});

describe("suggestFilters", () => {
  const suggest = (input: string) => suggestFilters(input, parse(input), voc);

  it("offers people, categories and apps for the word being typed", () => {
    expect(suggest("pricing sa").map((s) => s.label)).toEqual(["With Sam Lee"]);
    expect(suggest("pricing sa")[0]?.input).toBe('pricing with:"Sam Lee" ');
    expect(suggest("wo").map((s) => s.label)).toEqual(["In Work"]);
    expect(suggest("zo").map((s) => s.label)).toEqual(["On Zoom"]);
    expect(suggest("noti").map((s) => s.label)).toEqual(["Imported from Notion"]);
  });

  it("offers tags after a #", () => {
    expect(suggest("#pr").map((s) => s.input)).toEqual(["#pricing "]);
  });

  it("stays quiet after a space or once a filter is read", () => {
    expect(suggest("sa ")).toEqual([]);
    expect(suggest("with sam")).toEqual([]);
    expect(suggest("s")).toEqual([]);
  });
});
