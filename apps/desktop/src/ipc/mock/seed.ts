/** Demo data: a small product team's notes, two recorded meetings, people, a thread and tasks. */
import type {
  AiAction,
  Category,
  Message,
  Note,
  NoteKind,
  NoteVersion,
  Proposal,
  Segment,
  Speaker,
  Task,
  TaskRun,
  Thread,
  Transcript,
} from "@/ipc/types";
import { defaultSettings, defaultSttModels, idleRecording, type MockState } from "./state";
import { weekStart } from "./swears";
import { ago, iso, snippetFrom, time } from "./util";

export type SeedKind = "demo" | "empty";

export function createState(seed: SeedKind, now: number = Date.now()): MockState {
  const base: MockState = {
    notes: new Map(),
    categories: new Map(),
    versions: [],
    transcripts: new Map(),
    people: [],
    settings: defaultSettings(seed === "demo"),
    threads: [],
    messages: [],
    proposals: [],
    aiActions: [],
    undoBodies: new Map(),
    pastMeetings: [],
    swearCoins: [],
    swearEmptied: null,
    tasks: [defaultNotesReview(now)],
    taskRuns: [],
    // A fresh install has no speech model yet (onboarding downloads it).
    sttModels: defaultSttModels().map((m) => (seed === "empty" ? { ...m, downloaded: false } : m)),
    permissions: {
      microphone: seed === "demo" ? "granted" : "not_determined",
      system_audio: "not_determined",
      accessibility: "not_determined",
    },
    providerKeys: [],
    recording: idleRecording(),
  };
  if (seed === "empty") return base;
  seedDemo(base, now);
  return base;
}

function defaultNotesReview(now: number): Task {
  return {
    id: "task_notes_review",
    name: "Notes review",
    prompt:
      "Look through my notes. Suggest notes to merge when they cover the same topic, content I could trim, and notes that look stale or empty. Explain each suggestion in one sentence.",
    scope: { type: "all_notes" },
    schedule: { type: "every_n_days", days: 3, hour: 9, minute: 0 },
    next_run_at: iso(now + 2 * time.DAY),
    enabled: true,
    is_default: true,
    last_run: null,
    pending_count: 0,
  };
}

// ───────── Note bodies ─────────

const Q3_BODY = `## Summary

The team agreed to ship the shorter onboarding before the pricing change. Setup drops from five steps to three, and email verification moves after the first note. Priya owns the rollout plan.

## Decisions

- Ship the three-step onboarding **before** the pricing change. [14:02]
- Email verification moves after the first note is created. [14:20]
- Keep the \`skip_onboarding\` flag for the sales demo build. [15:01]

## Action items

- [x] Send the pricing deck to Marco (You, today)
- [ ] Share the rollout plan (Priya, Thursday)
- [ ] Book five usability sessions for step 3 (Marco, before Thursday)
- [ ] Draft the follow-up email to Marco (You, Friday)

## Open questions

- Do we need a migration path for teams that are halfway through the old flow?
- Who signs off on the new empty state copy?

> "If we lose them at email verification, nothing after it matters." Marco, 14:22

Related: [[Onboarding review]], [[Pricing interview, Marco]] and the [[Rollout plan]]. #roadmap
`;

const PRICING_BODY = `## Summary

Marco walked through how his team buys tools. Seat-based pricing feels punishing for occasional users; he would rather pay for active members. The annual discount matters less than a clear upgrade path.

## Decisions

- Test an *active member* price with three design partners. [24:10]
- Keep the free plan, but cap recorded hours rather than notes. [29:40]

## Action items

- [ ] Write up the active-member model with two worked examples (You, next Tuesday)
- [ ] Ask finance whether monthly true-ups are possible (Priya)

## Open questions

- How do we count a member who only reads notes?
- Is 20 recorded hours a month enough for the free plan?

Follow-up to [[Q3 planning sync]] with [[Marco Diaz]]. #pricing #customers/interviews
`;

const ONBOARDING_BODY = `# Onboarding review

Where people drop off in the current five-step setup, from last month's funnel.

| Step | Started | Finished | Drop-off |
| --- | --- | --- | --- |
| Create account | 1,240 | 1,198 | 3% |
| Verify email | 1,198 | 802 | 33% |
| Grant microphone | 802 | 751 | 6% |
| First recording | 751 | 512 | 32% |

## What we learned

1. Email verification is the biggest leak. Most people never come back to the email.
2. People who grant the microphone *and* record within 10 minutes stay for a month.
3. The permissions copy reads like a warning. It should say what Cilica does with the audio.

What would make someone record a second meeting in the first week?

## Next

- [ ] Move verification after the first note
- [ ] Rewrite the permission rows ("Audio stays on this Mac")
- [x] Share the funnel with the team

Feeds into [[Q3 planning sync]]. #roadmap
`;

const SEARCH_BODY = `# Search ranking notes

FTS5 with \`bm25()\` gets us most of the way. Titles should outrank bodies, and transcripts come last.

\`\`\`sql
SELECT note_id, bm25(notes_fts, 10.0, 4.0, 1.0) AS rank
FROM notes_fts
WHERE notes_fts MATCH ?
ORDER BY rank
LIMIT 20;
\`\`\`

- Prefix matching on the last term so results appear while typing.
- Highlight with \`highlight()\`, but cap excerpts at about 90 characters.
- Recency breaks ties.

Should meeting titles count more than note titles? Do people search transcripts by speaker?

Compare with the onboarding review numbers before we change ranking.
`;

const READING_BODY = `# Reading list

1. [Shape Up](https://basecamp.com/shapeup) by Ryan Singer
2. *The Mom Test* by Rob Fitzpatrick
3. [Local-first software](https://www.inkandswitch.com/local-first/) by Ink & Switch
4. *Designing Data-Intensive Applications* by Martin Kleppmann

Next up: something that is not about work.

Notes so far: [[Book notes: The Mom Test]].
`;

const INBOX_BODY = `Things to sort out this week.

- [ ] Renew the domain before the 20th
- [x] Reply to the accountant
- [ ] Book the dentist
- [ ] Move the Lisbon notes into Personal
`;

const RELEASE_BODY = `# Release checklist 0.4

- [x] Bump the version in \`tauri.conf.json\`
- [x] Run the full test suite
- [ ] Notarise the macOS build
- [ ] Write the release notes
- [ ] Post in the beta channel

\`\`\`bash
bun run build
xcrun notarytool submit Cilica.dmg --wait
\`\`\`

Ship it once notarisation comes back clean.
`;

const DESIGN_LEAD_BODY = `# Design lead, round 2

**Candidate:** Lena Brandt · **Panel:** You, Priya, Sam

## Strengths

- Clear thinking about systems: walked through how she built tokens for two products.
- Strong on motion. Showed prototypes, not just stills.
- Asked good questions about how we decide what to cut.

## Concerns

- Less experience with desktop apps. Most of her work is web and iOS.
- Wants to manage soon; we need hands-on work for the next year.

## Recommendation

Move to the final round with a take-home on the sidebar. *Strong yes* from Priya, *yes* from Sam.

Loop: [[Interview loop template]] #hiring
`;

const LOOP_BODY = `# Interview loop template

1. **Intro call** (30 min): motivation, logistics, salary range.
2. **Portfolio review** (60 min): two projects, one they are proud of and one that went wrong.
3. **Working session** (90 min): a real problem from our backlog.
4. **Team chat** (45 min): no evaluation, just questions both ways.

> Write feedback within 24 hours, before talking to the rest of the panel.

Used for [[Design lead, round 2]]. #hiring
`;

const SAM_BODY = `# Weekly 1:1 with Sam

## Sam's topics

- Feels stretched between the sidebar rebuild and support rotation.
- Wants more time with the Rust side; curious about the audio pipeline.

## My topics

- Thanks for the drag-and-drop work. The drop animation is lovely.
- Q3 goals: one owner per surface.

## Follow-ups

- [ ] Take Sam off support rotation for two weeks
- [ ] Pair Sam with the audio work on Thursdays

Context: [[Q3 planning sync]] with [[Sam Lee]]. #one-on-one
`;

const PRIYA_BODY = `# 1:1 with Priya

- Rollout plan for onboarding is on track for Thursday.
- She would like a second designer on pricing by October.
- Asked for feedback on her last review: *specific and kind*, keep doing it.

- [ ] Draft the job post for a second product designer

See [[Onboarding review]] and [[Pricing interview, Marco]]. #one-on-one
`;

const LISBON_BODY = `# Trip to Lisbon

Dates: 14 to 19 October.

## To book

- [x] Flights
- [ ] Apartment in Alfama or Príncipe Real
- [ ] Day trip to Sintra (go early, before the buses)

## Eat

- Pastéis de nata at Manteigaria
- Grilled sardines anywhere near the river
- Coffee at Hello, Kristof

#travel
`;

const MOM_TEST_BODY = `# Book notes: The Mom Test

> Talk about their life instead of your idea.

- Ask about specifics in the past, not opinions about the future.
- Compliments are not data. Commitments are.
- Keep conversations casual; skip the meeting format when you can.

The best question so far: **"What did you try last time this happened?"**

Useful before the next [[Pricing interview, Marco]].
`;

const OLD_ROADMAP_BODY = `# Roadmap draft (old)

- Calendar integration
- Cloud sync
- Sharing

Superseded by the Q3 plan.
`;

// ───────── Seed ─────────

interface NoteSeed {
  id: string;
  title: string;
  body: string;
  category: string | null;
  position: number;
  created: { days?: number; hours?: number; minutes?: number };
  updated: { days?: number; hours?: number; minutes?: number };
  kind?: NoteKind;
  meetingApp?: string;
  snippet?: string;
  durationMin?: number;
  deleted?: { days?: number; hours?: number };
}

function seedDemo(state: MockState, now: number): void {
  const categories: Category[] = [
    { id: "cat_product", name: "Product", position: 1, collapsed: false },
    { id: "cat_hiring", name: "Hiring", position: 2, collapsed: false },
    { id: "cat_one_on_ones", name: "1:1s", position: 3, collapsed: false },
    { id: "cat_personal", name: "Personal", position: 4, collapsed: true },
  ];
  for (const c of categories) state.categories.set(c.id, c);

  const notes: NoteSeed[] = [
    {
      id: "note_inbox",
      title: "Inbox, Monday",
      body: INBOX_BODY,
      category: null,
      position: 1,
      created: { hours: 3 },
      updated: { minutes: 25 },
    },
    {
      id: "note_release",
      title: "Release checklist 0.4",
      body: RELEASE_BODY,
      category: null,
      position: 2,
      created: { days: 1 },
      updated: { hours: 20 },
    },
    {
      id: "note_reading",
      title: "Reading list",
      body: READING_BODY,
      category: null,
      position: 3,
      created: { days: 20 },
      updated: { days: 6 },
    },
    {
      id: "note_q3",
      title: "Q3 planning sync",
      body: Q3_BODY,
      category: "cat_product",
      position: 1,
      created: { hours: 4 },
      updated: { hours: 3 },
      kind: "meeting",
      meetingApp: "Zoom",
      durationMin: 42,
      snippet: "Ship the three-step onboarding before the pricing change; Priya owns the rollout.",
    },
    {
      id: "note_onboarding",
      title: "Onboarding review",
      body: ONBOARDING_BODY,
      category: "cat_product",
      position: 2,
      created: { days: 3 },
      updated: { days: 1 },
    },
    {
      id: "note_pricing",
      title: "Pricing interview, Marco",
      body: PRICING_BODY,
      category: "cat_product",
      position: 3,
      created: { days: 2 },
      updated: { days: 2 },
      kind: "meeting",
      meetingApp: "Google Meet",
      durationMin: 31,
      snippet: "Marco prefers paying for active members over seats; test it with three partners.",
    },
    {
      id: "note_search",
      title: "Search ranking notes",
      body: SEARCH_BODY,
      category: "cat_product",
      position: 4,
      created: { days: 14 },
      updated: { days: 9 },
    },
    {
      id: "note_design_lead",
      title: "Design lead, round 2",
      body: DESIGN_LEAD_BODY,
      category: "cat_hiring",
      position: 1,
      created: { days: 5 },
      updated: { days: 5 },
    },
    {
      id: "note_loop",
      title: "Interview loop template",
      body: LOOP_BODY,
      category: "cat_hiring",
      position: 2,
      created: { days: 40 },
      updated: { days: 15 },
    },
    {
      id: "note_sam",
      title: "Weekly 1:1 with Sam",
      body: SAM_BODY,
      category: "cat_one_on_ones",
      position: 1,
      created: { days: 1 },
      updated: { days: 1 },
    },
    {
      id: "note_priya",
      title: "1:1 with Priya",
      body: PRIYA_BODY,
      category: "cat_one_on_ones",
      position: 2,
      created: { days: 8 },
      updated: { days: 8 },
    },
    {
      id: "note_lisbon",
      title: "Trip to Lisbon",
      body: LISBON_BODY,
      category: "cat_personal",
      position: 1,
      created: { days: 12 },
      updated: { days: 4 },
    },
    {
      id: "note_mom_test",
      title: "Book notes: The Mom Test",
      body: MOM_TEST_BODY,
      category: "cat_personal",
      position: 2,
      created: { days: 30 },
      updated: { days: 30 },
    },
    {
      id: "note_old_roadmap",
      title: "Roadmap draft (old)",
      body: OLD_ROADMAP_BODY,
      category: "cat_product",
      position: 9,
      created: { days: 60 },
      updated: { days: 45 },
      deleted: { days: 5 },
    },
    {
      id: "note_big_idea",
      title: "Big idea",
      body: "",
      category: null,
      position: 7,
      created: { days: 41 },
      updated: { days: 41 },
    },
    {
      id: "note_weekend",
      title: "Weekend plans",
      body: "We definitely need a plan for Saturday. Definitely book the train early.\n\nThen maybe lunch by the river and after that we could",
      category: "cat_personal",
      position: 3,
      created: { days: 6 },
      updated: { hours: 20 },
    },
    {
      id: "note_half_thought",
      title: "Half a thought",
      body: "Something about how onboarding and pricing are really the same problem because",
      category: null,
      position: 8,
      created: { days: 4 },
      updated: { days: 4 },
    },
    {
      id: "note_scratch",
      title: "",
      body: "asdf\n",
      category: null,
      position: 9,
      created: { days: 1 },
      updated: { days: 1 },
      deleted: { hours: 22 },
    },
  ];

  for (const n of notes) {
    const createdAt = ago(now, n.created);
    const kind = n.kind ?? "manual";
    const startedMs = Date.parse(createdAt);
    const note: Note = {
      id: n.id,
      title: n.title,
      body_md: n.body,
      snippet: n.snippet ?? snippetFrom(n.body),
      category_id: n.category,
      kind,
      meeting_app: n.meetingApp ?? null,
      started_at: kind === "meeting" ? createdAt : null,
      ended_at: kind === "meeting" ? iso(startedMs + (n.durationMin ?? 30) * time.MINUTE) : null,
      position: n.position,
      created_at: createdAt,
      updated_at: ago(now, n.updated),
      deleted_at: n.deleted ? ago(now, n.deleted) : null,
    };
    state.notes.set(note.id, note);
  }

  state.people = [
    {
      id: "person_you",
      name: "You",
      color_index: 1,
      sample_count: 4,
      meeting_count: 12,
      created_at: ago(now, { days: 120 }),
      is_self: true,
    },
    {
      id: "person_priya",
      name: "Priya Shah",
      color_index: 6,
      sample_count: 23,
      meeting_count: 23,
      created_at: ago(now, { days: 90 }),
      is_self: false,
    },
    {
      id: "person_marco",
      name: "Marco Diaz",
      color_index: 2,
      sample_count: 9,
      meeting_count: 9,
      created_at: ago(now, { days: 60 }),
      is_self: false,
    },
    {
      id: "person_sam",
      name: "Sam Lee",
      color_index: 3,
      sample_count: 14,
      meeting_count: 14,
      created_at: ago(now, { days: 75 }),
      is_self: false,
    },
  ];

  state.transcripts.set("note_q3", q3Transcript());
  state.transcripts.set("note_pricing", pricingTranscript());

  state.versions = seedVersions(state, now);
  state.swearCoins = seedSwearCoins(now);
  state.aiActions = seedAiActions(now);
  state.pastMeetings = [30, 31, 41].map((mins, i) => ({
    id: `past_q3_${i + 1}`,
    title: "Q3 planning sync",
    held_at: ago(now, { days: 28 - i * 7 }),
    duration_ms: mins * 60_000,
  }));
  state.undoBodies.set("act_seed_undone_3", {
    note_id: "note_onboarding",
    before_md: "We're fixing onboarding first.",
    after_md: "This quarter, we will strategically prioritise onboarding synergies.",
  });
  const { threads, messages } = seedThread(now);
  state.threads = threads;
  state.messages = messages;

  state.tasks.push({
    id: "task_weekly_digest",
    name: "Weekly digest",
    prompt:
      "Every Friday, summarise what changed in my notes this week and list open action items with owners.",
    scope: { type: "last_days", days: 7 },
    schedule: { type: "weekly", weekday: 4, hour: 16, minute: 0 },
    next_run_at: iso(now + 3 * time.DAY),
    enabled: true,
    is_default: false,
    last_run: null,
    pending_count: 0,
  });
  const run: TaskRun = {
    id: "run_seed_review",
    task_id: "task_notes_review",
    started_at: ago(now, { days: 1, minutes: 2 }),
    finished_at: ago(now, { days: 1 }),
    status: "succeeded",
    summary: "Found 2 notes that cover the same topic and 1 note that looks stale.",
    error: null,
  };
  state.taskRuns.push(run);
  state.proposals.push(...seedTaskProposals(run, now, state));
}

function speaker(
  noteId: string,
  id: string,
  label: string,
  color: number,
  personId: string | null,
  isSelf = false,
): Speaker {
  return { id, note_id: noteId, label, person_id: personId, color_index: color, is_self: isSelf };
}

function segments(noteId: string, lines: [string, "mic" | "system", number, string][]): Segment[] {
  return lines.map(([speakerId, channel, startSec, text], i) => {
    const next = lines[i + 1];
    const endSec = next ? Math.max(startSec + 2, next[2] - 1) : startSec + 6;
    return {
      id: `${noteId}_seg_${i + 1}`,
      note_id: noteId,
      speaker_id: speakerId,
      channel,
      start_ms: startSec * 1000,
      end_ms: endSec * 1000,
      text,
      is_final: true,
    };
  });
}

/** Swears typed into notes: a few this week and a history for Settings › Swear jar. */
function seedSwearCoins(now: number): MockState["swearCoins"] {
  const words = ["fuck", "shit", "bloody", "fuck", "damn", "bollocks"];
  const coins: MockState["swearCoins"] = [];
  const add = (at: number, word: string) =>
    coins.push({
      id: `coin_${coins.length + 1}`,
      note_id: "note_onboarding",
      word,
      at: new Date(at).toISOString(),
    });
  // This week, in the last day or so.
  ["fuck", "shit", "fuck", "bollocks", "shit", "fuck"].forEach((w, i) => {
    add(Math.max(weekStart(now), now - (i + 1) * 3 * 3600_000), w);
  });
  // The eight weeks before, newest first; the oldest is the record.
  [17, 20, 14, 31, 9, 26, 18, 41].forEach((n, back) => {
    const day = weekStart(now) - (back + 1) * 7 * 24 * 3600_000 + 2 * 24 * 3600_000;
    for (let i = 0; i < n; i++) add(day + i * 60_000, words[i % words.length] as string);
  });
  return coins;
}

function q3Transcript(): Transcript {
  const id = "note_q3";
  return {
    note_id: id,
    speakers: [
      speaker(id, "spk_q3_you", "You", 1, null, true),
      speaker(id, "spk_q3_priya", "Priya Shah", 6, "person_priya"),
      speaker(id, "spk_q3_marco", "Marco Diaz", 2, "person_marco"),
      speaker(id, "spk_q3_s3", "Speaker 3", 8, null),
    ],
    segments: segments(id, [
      [
        "spk_q3_you",
        "mic",
        12,
        "Okay, I think everyone's here. Let's start with onboarding and leave pricing for the second half.",
      ],
      [
        "spk_q3_you",
        "mic",
        20,
        "So basically the setup has five steps and people basically give up around the third one.",
      ],
      [
        "spk_q3_you",
        "mic",
        29,
        "Basically we lose a third of them at verification, which is the step nobody asked for.",
      ],
      [
        "spk_q3_you",
        "mic",
        38,
        "And the fix is basically to move verification after the first note, so they see the point.",
      ],
      [
        "spk_q3_you",
        "mic",
        47,
        "I mean the numbers are basically the same every month, so this is not a blip.",
      ],
      [
        "spk_q3_priya",
        "system",
        828,
        "Before we get into pricing, can we look at where people drop off in setup?",
      ],
      [
        "spk_q3_marco",
        "system",
        835,
        "To be fair, step three. Email verification. About a third of new accounts stop there.",
      ],
      ["spk_q3_you", "mic", 839, "Oh, for fuck’s sake. A third of them?"],
      [
        "spk_q3_priya",
        "system",
        842,
        "So the plan is to cut the flow down to three steps and move verification after the first note.",
      ],
      [
        "spk_q3_marco",
        "system",
        862,
        "To be fair, if we lose them at email verification, nothing after it matters.",
      ],
      ["spk_q3_marco", "system", 867, "It’s a shit flow. It always was."],
      ["spk_q3_s3", "system", 871, "Can we test that with five people before Thursday?"],
      [
        "spk_q3_you",
        "mic",
        880,
        "Yes. Marco, can you book the sessions? I'll send you the bloody pricing deck today.",
      ],
      ["spk_q3_marco", "system", 889, "Sure. To be fair, I'll find five people from the waitlist."],
      [
        "spk_q3_priya",
        "system",
        901,
        "And I'll have the rollout plan ready on Thursday, before the road map review. We keep the skip onboarding flag for the sales demo.",
      ],
      ["spk_q3_you", "mic", 915, "Great. Last thing: who signs off on the new empty state copy?"],
      ["spk_q3_priya", "system", 924, "Let's decide that next week once we've seen the sessions."],
    ]),
    duration_ms: 42 * 60_000,
    has_audio: true,
    timed: true,
    quotes: [
      {
        at_ms: 901_000,
        text: "We keep the skip onboarding flag for the sales demo.",
      },
      { at_ms: 915_000, text: "Who signs off on the new empty state copy?" },
    ],
  };
}

function pricingTranscript(): Transcript {
  const id = "note_pricing";
  return {
    note_id: id,
    speakers: [
      speaker(id, "spk_pr_you", "You", 1, null, true),
      speaker(id, "spk_pr_marco", "Marco Diaz", 2, "person_marco"),
    ],
    segments: segments(id, [
      [
        "spk_pr_you",
        "mic",
        5,
        "Thanks for making time. I'd love to hear how your team decides to pay for a tool.",
      ],
      [
        "spk_pr_marco",
        "system",
        14,
        "Usually someone tries it alone for a few weeks, then asks for a team plan.",
      ],
      [
        "spk_pr_marco",
        "system",
        26,
        "Seat pricing is what kills it. Half the team opens notes once a month.",
      ],
      ["spk_pr_you", "mic", 39, "Would paying only for active members change that?"],
      [
        "spk_pr_marco",
        "system",
        47,
        "Completely. I'd rather pay for people who actually record meetings.",
      ],
      [
        "spk_pr_marco",
        "system",
        63,
        "The annual discount matters less than knowing what happens when we grow.",
      ],
      ["spk_pr_you", "mic", 78, "And the free plan, would a limit on recorded hours feel fair?"],
      ["spk_pr_marco", "system", 86, "Hours, yes. Limiting the number of notes would feel petty."],
    ]),
    duration_ms: 31 * 60_000,
    has_audio: false,
    timed: true,
    quotes: [
      {
        at_ms: 26_000,
        text: "Seat pricing is what kills it. Half the team opens notes once a month.",
      },
      { at_ms: 86_000, text: "Limiting the number of notes would feel petty." },
    ],
  };
}

function seedVersions(state: MockState, now: number): NoteVersion[] {
  const q3 = state.notes.get("note_q3");
  const onboarding = state.notes.get("note_onboarding");
  const sam = state.notes.get("note_sam");
  const out: NoteVersion[] = [];
  const weekend = state.notes.get("note_weekend");
  if (weekend) {
    const v = (id: string, body_md: string, hours: number): NoteVersion => ({
      id,
      note_id: weekend.id,
      title: weekend.title,
      body_md,
      author: "user",
      ai_action_id: null,
      created_at: ago(now, { hours }),
    });
    out.push(
      v("ver_weekend_1", "We definately need a plan for Saturday.", 30),
      v("ver_weekend_2", "We definitely need a plan for Saturday.", 28),
      v(
        "ver_weekend_3",
        "We definitely need a plan for Saturday. Definately book the train early.",
        26,
      ),
    );
  }
  // Cilica wrote most of the book notes: the version before its write held only the heading.
  const book = state.notes.get("note_mom_test");
  if (book) {
    out.push({
      id: "ver_mom_test_1",
      note_id: book.id,
      title: book.title,
      body_md: "# Book notes: The Mom Test\n",
      author: "ai",
      ai_action_id: "act_seed_book",
      created_at: ago(now, { days: 9 }),
    });
  }
  if (q3) {
    out.push(
      {
        id: "ver_q3_jottings",
        note_id: q3.id,
        title: q3.title,
        body_md:
          "- verification is the big drop\n- Priya: rollout plan Thu\n- ask Marco about usability sessions\n",
        author: "user",
        ai_action_id: null,
        created_at: ago(now, { hours: 3, minutes: 20 }),
      },
      {
        id: "ver_q3_summary",
        note_id: q3.id,
        title: q3.title,
        body_md: Q3_BODY.replace("- [x] Send the pricing deck", "- [ ] Send the pricing deck"),
        author: "ai",
        ai_action_id: "act_seed_summary",
        created_at: ago(now, { hours: 3, minutes: 5 }),
      },
    );
  }
  if (onboarding) {
    out.push(
      {
        id: "ver_onb_1",
        note_id: onboarding.id,
        title: onboarding.title,
        body_md:
          "# Onboarding review\n\nWhere people drop off in the current five-step setup.\n\n- Verify email: 33% drop-off\n- First recording: 32% drop-off\n",
        author: "user",
        ai_action_id: null,
        created_at: ago(now, { days: 3 }),
      },
      {
        id: "ver_onb_2",
        note_id: onboarding.id,
        title: onboarding.title,
        body_md: ONBOARDING_BODY.replace(
          /## Next[\s\S]*$/,
          "## Next\n\n- [ ] Move verification after the first note\n",
        ),
        author: "user",
        ai_action_id: null,
        created_at: ago(now, { days: 2 }),
      },
    );
  }
  if (sam) {
    out.push({
      id: "ver_sam_1",
      note_id: sam.id,
      title: sam.title,
      body_md:
        "# Weekly 1:1 with Sam\n\n## Sam's topics\n\n- Feels stretched between the sidebar rebuild and support rotation.\n",
      author: "user",
      ai_action_id: null,
      created_at: ago(now, { days: 1, hours: 1 }),
    });
  }
  return out;
}

function seedAiActions(now: number): AiAction[] {
  const base = { thread_id: null, error: null, proposal_id: null };
  return [
    {
      ...base,
      id: "act_seed_summary",
      turn_id: "turn_seed_summary",
      tool: "edit_note",
      tier: "edit",
      args_json: JSON.stringify({ note_id: "note_q3", section: "Summary" }),
      summary: "Wrote the summary for Q3 planning sync",
      status: "applied",
      can_undo: true,
      created_at: ago(now, { hours: 3, minutes: 5 }),
    },
    {
      ...base,
      id: "act_seed_search",
      thread_id: "thread_seed",
      turn_id: "turn_seed_1",
      tool: "search_notes",
      tier: "read",
      args_json: JSON.stringify({ query: "onboarding" }),
      summary: "Searched 3 notes",
      status: "applied",
      can_undo: false,
      created_at: ago(now, { hours: 2 }),
    },
    {
      ...base,
      id: "act_seed_move",
      turn_id: "turn_seed_move",
      tool: "move_note",
      tier: "additive",
      args_json: JSON.stringify({ note_id: "note_pricing", category_id: "cat_product" }),
      summary: "Moved Pricing interview, Marco to Product",
      status: "applied",
      can_undo: true,
      created_at: ago(now, { days: 2 }),
    },
    ...[
      ["act_seed_undone_1", "Tidied the headings in Reading list", { days: 4 }],
      ["act_seed_undone_2", "Shortened Trip to Lisbon", { days: 3 }],
      ["act_seed_undone_3", "Rewrote the intro of Onboarding review", { hours: 5 }],
    ].map(
      ([id, summary, when]): AiAction => ({
        ...base,
        id: id as string,
        turn_id: `turn_${id}`,
        tool: "edit_note",
        tier: "edit",
        args_json: "{}",
        summary: summary as string,
        status: "undone",
        can_undo: false,
        created_at: ago(now, when as { days?: number; hours?: number }),
      }),
    ),
  ];
}

function seedThread(now: number): { threads: Thread[]; messages: Message[] } {
  const thread: Thread = {
    id: "thread_seed",
    title: "Onboarding decisions",
    created_at: ago(now, { hours: 2, minutes: 1 }),
    updated_at: ago(now, { hours: 2 }),
  };
  const messages: Message[] = [
    {
      id: "msg_seed_user",
      thread_id: thread.id,
      turn_id: "turn_seed_1",
      role: "user",
      parts: [{ type: "text", text: "What did we decide about onboarding, and who owns it?" }],
      created_at: ago(now, { hours: 2, minutes: 1 }),
    },
    {
      id: "msg_seed_reply",
      thread_id: thread.id,
      turn_id: "turn_seed_1",
      role: "assistant",
      parts: [
        {
          type: "tool",
          action: {
            id: "act_seed_search",
            thread_id: thread.id,
            turn_id: "turn_seed_1",
            tool: "search_notes",
            tier: "read",
            args_json: JSON.stringify({ query: "onboarding" }),
            summary: "Searched 3 notes",
            status: "applied",
            error: null,
            can_undo: false,
            proposal_id: null,
            created_at: ago(now, { hours: 2 }),
          },
        },
        {
          type: "text",
          text: "You agreed to ship the three-step onboarding before the pricing change, with email verification moved after the first note. Priya owns the rollout plan and shares it on Thursday (Q3 planning sync, 15:01).",
        },
      ],
      created_at: ago(now, { hours: 2 }),
    },
  ];
  return { threads: [thread], messages };
}

/**
 * Pending suggestions from a Notes review run: a merge, a clean-up, and (when the notes allow)
 * a trim of the onboarding note and a new note, so every report group has something in it.
 */
export function seedTaskProposals(run: TaskRun, now: number, state?: MockState): Proposal[] {
  const createdAt = run.finished_at ?? iso(now);
  const extra: Proposal[] = [];
  const onboarding = state?.notes.get("note_onboarding");
  if (onboarding && !onboarding.deleted_at) {
    const lines = onboarding.body_md.split("\n");
    const cut = lines.findIndex((l) => /^\s*(\d+\.|[-*])\s/.test(l) && l.length > 60);
    if (cut >= 0) {
      extra.push({
        id: `prop_${run.id}_trim`,
        source: "task",
        thread_id: null,
        task_run_id: run.id,
        tier: "edit",
        kind: {
          type: "edit_note",
          note: { id: onboarding.id, title: onboarding.title },
          before_md: onboarding.body_md,
          after_md: lines.filter((_, i) => i !== cut).join("\n"),
        },
        reason: "This point repeats what the Q3 planning sync already decided.",
        status: "pending",
        created_at: createdAt,
        resolved_at: null,
      });
    }
  }
  if (state) {
    extra.push({
      id: `prop_${run.id}_create`,
      source: "task",
      thread_id: null,
      task_run_id: run.id,
      tier: "additive",
      kind: {
        type: "create_note",
        title: "Open action items",
        body_md:
          "- [ ] Book five usability sessions (Marco, Thursday)\n- [ ] Share the rollout plan (Priya, Thursday)\n- [ ] Decide who signs off on empty state copy\n",
        category_id: null,
        category_name: null,
      },
      reason: "Three notes end with open items; one list makes them easier to follow up.",
      status: "pending",
      created_at: createdAt,
      resolved_at: null,
    });
  }
  return [
    {
      id: `prop_${run.id}_merge`,
      source: "task",
      thread_id: null,
      task_run_id: run.id,
      tier: "destructive",
      kind: {
        type: "merge_notes",
        sources: [
          { id: "note_priya", title: "1:1 with Priya" },
          { id: "note_q3", title: "Q3 planning sync" },
        ],
        merged_title: "Onboarding rollout",
        merged_md:
          "# Onboarding rollout\n\n- Ship the three-step onboarding before the pricing change.\n- Priya owns the rollout plan, due Thursday.\n- Marco books five usability sessions for step 3.\n",
      },
      reason: "Both notes track the onboarding rollout and repeat the same owners and dates.",
      status: "pending",
      created_at: createdAt,
      resolved_at: null,
    },
    {
      id: `prop_${run.id}_trash`,
      source: "task",
      thread_id: null,
      task_run_id: run.id,
      tier: "destructive",
      kind: { type: "trash_notes", notes: [{ id: "note_reading", title: "Reading list" }] },
      reason:
        "Reading list has not changed in weeks and every item is already linked from Personal.",
      status: "pending",
      created_at: createdAt,
      resolved_at: null,
    },
    ...extra,
  ];
}
