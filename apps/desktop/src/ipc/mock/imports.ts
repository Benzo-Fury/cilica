/**
 * Settings › Import in the mock: an Obsidian vault and Anarlog "found" on this computer, Granola
 * installed, keys that work when they start with the provider's usual prefix (or are long enough),
 * previews made up from the input, and imports that add notes a few at a time with progress events.
 */
import type {
  ImportDetection,
  ImportInput,
  ImportKeyStatus,
  ImportOptions,
  ImportPreview,
  ImportProgress,
  ImportRun,
  Note,
} from "@/ipc/types";
import type { MockContext } from "./state";
import { Timers } from "./timers";
import { appError, iso, newId, time } from "./util";

interface PendingScan {
  provider: string;
  origin: string;
  titles: { title: string; meeting: boolean; folder: string | null; date: string }[];
}

const FOUND: Record<string, Omit<ImportDetection, "has_key" | "provider">> = {
  obsidian: {
    app_installed: true,
    note: null,
    locations: [
      {
        label: "Sample Vault",
        path: "~/Documents/Obsidian Vault/Sample Vault",
        notes: 42,
        meetings: 0,
        transcripts: 0,
        attachments: 7,
        unsupported: [
          {
            kind: "Canvases",
            count: 2,
            reason:
              "Margin has no canvas view, so canvas boards are left out. The notes on them are imported on their own.",
          },
        ],
      },
    ],
  },
  anarlog: {
    app_installed: true,
    note: null,
    locations: [
      {
        label: "Anarlog on this computer",
        path: "~/Library/Application Support/anarlog/app.db",
        notes: 0,
        meetings: 4,
        transcripts: 3,
        attachments: 0,
        unsupported: [
          {
            kind: "Recordings",
            count: 4,
            reason: "Audio stays in Anarlog. Margin imports the notes, summaries and transcripts.",
          },
        ],
      },
    ],
  },
  granola: {
    app_installed: true,
    locations: [],
    note: "Granola encrypts the notes it keeps on this Mac, so Margin reads them through your API key or a CSV export instead.",
  },
  apple_notes: {
    app_installed: true,
    locations: [],
    note: "Apple Notes keeps its notes where only it can read them. When you check, macOS asks whether Margin may read Notes; choose OK.",
  },
};

const NAMES: Record<string, string> = {
  obsidian: "Obsidian",
  anarlog: "Anarlog",
  granola: "Granola",
  fireflies: "Fireflies",
  fathom: "Fathom",
  notion: "Notion",
  evernote: "Evernote",
  apple_notes: "Apple Notes",
};

export function importsCommands(ctx: MockContext) {
  const { state } = ctx;
  const timers = new Timers();
  const keys = new Set<string>();
  const scans = new Map<string, PendingScan>();
  // Runs of this session, for Cancel and Undo. Nothing is kept once the app closes.
  const runs: ImportRun[] = [];
  const created = new Map<string, string[]>();
  // Which note each source item became; it only counts while that note is outside Trash.
  const sources = new Map<string, string>();
  const isHere = (key: string) => {
    const id = sources.get(key);
    const note = id ? state.notes.get(id) : undefined;
    return Boolean(note && !note.deleted_at);
  };

  const detection = (provider: string): ImportDetection => {
    const found = FOUND[provider] ?? { app_installed: false, locations: [], note: null };
    return { provider, has_key: keys.has(provider), ...found };
  };

  const progress = (p: ImportProgress) => ctx.emit({ type: "import_progress", progress: p });

  return {
    dispose: () => timers.clear(),

    importDetect(provider: string): ImportDetection {
      return detection(provider);
    },

    importDetectAll(): ImportDetection[] {
      return [...new Set([...Object.keys(FOUND), ...keys])].map(detection);
    },

    importSaveKey(provider: string, key: string): ImportKeyStatus {
      const k = key.trim();
      if (!k) throw appError("invalid", "Paste the API key first.");
      if (k.length < 12) {
        const name = NAMES[provider] ?? "That app";
        return {
          ok: false,
          problem: `${name} didn't accept the API key. Copy it again from ${name}'s settings and paste it here.`,
          account: null,
        };
      }
      keys.add(provider);
      return { ok: true, problem: null, account: "Connected as sam@example.com" };
    },

    importClearKey(provider: string): null {
      keys.delete(provider);
      return null;
    },

    importScan(provider: string, input: ImportInput): ImportPreview {
      const now = Date.now();
      const name = NAMES[provider] ?? provider;
      let origin: string;
      let titles: PendingScan["titles"];
      const meetingish = ![
        "obsidian",
        "notion",
        "evernote",
        "apple_notes",
        "bear",
        "logseq",
        "google_keep",
        "joplin",
      ].includes(provider);
      switch (input.type) {
        case "paste": {
          if (!input.text.trim()) throw appError("invalid", "Paste something first.");
          origin = "Pasted text";
          titles = [
            {
              title: input.title ?? "Pasted transcript",
              meeting: input.text.includes(":"),
              folder: null,
              date: iso(now),
            },
          ];
          break;
        }
        case "account":
          if (!keys.has(provider)) throw appError("invalid", `Paste your ${name} API key first.`);
          origin = `your ${name} account`;
          titles = SAMPLE_MEETINGS.map((t, i) => ({
            title: t,
            meeting: true,
            folder: null,
            date: iso(now - (i + 1) * 2 * time.DAY),
          }));
          break;
        case "local":
          origin = FOUND[provider]?.locations[0]?.label ?? name;
          titles = (meetingish ? SAMPLE_MEETINGS : SAMPLE_NOTES).map((t, i) => ({
            title: t,
            meeting: meetingish,
            folder: meetingish ? null : i % 3 === 0 ? "Projects" : i % 3 === 1 ? "Journal" : null,
            date: iso(now - (i + 1) * 5 * time.DAY),
          }));
          break;
        case "paths":
          if (input.paths.length === 0)
            throw appError("invalid", "Choose a file or folder to import.");
          origin =
            input.paths.length === 1
              ? (input.paths[0]?.split("/").pop() ?? "1 item")
              : `${input.paths.length} items`;
          titles = (meetingish ? SAMPLE_MEETINGS : SAMPLE_NOTES).slice(0, 6).map((t, i) => ({
            title: t,
            meeting: meetingish,
            folder: meetingish ? null : i % 2 === 0 ? "Projects" : null,
            date: iso(now - (i + 1) * 3 * time.DAY),
          }));
          break;
      }
      const scanId = newId("scan");
      scans.set(scanId, { provider, origin, titles });
      const existing = titles.filter((t) => isHere(`${provider}:${t.title}`)).length;
      const meetings = titles.filter((t) => t.meeting).length;
      const dates = titles.map((t) => t.date).sort();
      return {
        scan_id: scanId,
        provider,
        origin,
        total: titles.length,
        notes: titles.length - meetings,
        meetings,
        transcripts: input.type === "account" ? 0 : meetings,
        attachments: meetingish ? 0 : 3,
        earliest: dates[0] ?? null,
        latest: dates[dates.length - 1] ?? null,
        includes: {
          summary: meetingish,
          my_notes: true,
          transcript: meetingish,
          speakers: meetingish,
          timestamps: meetingish && input.type !== "paste",
          attachments: !meetingish,
        },
        folders: [...new Set(titles.map((t) => t.folder).filter((f): f is string => f !== null))],
        speakers: meetingish
          ? [
              { name: "Sam Lee", meetings: meetings },
              { name: "Priya Shah", meetings: Math.max(1, meetings - 2) },
              { name: "Marco Bianchi", meetings: 2 },
            ]
          : [],
        existing,
        unsupported:
          provider === "obsidian" ? (FOUND.obsidian?.locations[0]?.unsupported ?? []) : [],
        warnings:
          input.type === "paths" && input.paths.some((p) => p.endsWith(".pdf"))
            ? ["export.pdf is not a format Margin reads. Export it again as .txt or .docx."]
            : [],
        samples: [...titles]
          .reverse()
          .slice(0, 5)
          .map((t) => ({
            title: t.title,
            date: t.date,
            is_meeting: t.meeting,
            excerpt:
              input.type === "account"
                ? ""
                : t.meeting
                  ? "We agreed to ship the beta on Friday and to check pricing with two customers first."
                  : "Ideas and links collected over the week.",
          })),
        details_pending: input.type === "account",
      };
    },

    importStart(scanId: string, options: ImportOptions): ImportRun {
      const scan = scans.get(scanId);
      if (!scan)
        throw appError(
          "not_found",
          "That preview has expired. Check the source again, then import.",
        );
      scans.delete(scanId);
      const run: ImportRun = {
        id: newId("import"),
        provider: scan.provider,
        origin: scan.origin,
        status: "running",
        imported: 0,
        skipped: 0,
        failed: 0,
        report: [],
        started_at: iso(),
        finished_at: null,
      };
      runs.unshift(run);
      created.set(run.id, []);
      const name = NAMES[scan.provider] ?? scan.provider;
      const categoryFor = (folder: string | null): string | null => {
        if (options.category_mode === "none") return null;
        const wanted = options.category_mode === "keep_folders" && folder ? folder : `From ${name}`;
        const existing = [...state.categories.values()].find(
          (c) => c.name.toLowerCase() === wanted.toLowerCase(),
        );
        if (existing) return existing.id;
        const id = newId("cat");
        const last = Math.max(0, ...[...state.categories.values()].map((c) => c.position ?? 0));
        state.categories.set(id, {
          id,
          name: wanted,
          position: Math.floor(last) + 1,
          collapsed: false,
        });
        return id;
      };
      const total = scan.titles.length;
      let i = 0;
      const step = () => {
        if (run.status !== "running") return;
        const item = scan.titles[i];
        if (!item) {
          run.status = run.failed > 0 ? "partial" : "succeeded";
          run.finished_at = iso();
          ctx.emit({
            type: "data_changed",
            scopes: ["notes", "categories"],
            note_id: null,
          });
          progress({
            run_id: run.id,
            provider: run.provider,
            phase: "done",
            done: total,
            total,
            current: null,
            run: { ...run },
          });
          return;
        }
        i += 1;
        const key = `${scan.provider}:${item.title}`;
        if (isHere(key)) {
          run.skipped += 1;
        } else {
          const categoryId = categoryFor(item.folder);
          const note: Note = {
            id: newId("note"),
            title: item.title,
            body_md: item.meeting
              ? "## Summary\n\nWe agreed to ship the beta on Friday and to check pricing with two customers first.\n\n## Action items\n\n- [ ] Send the deck to Priya\n- [ ] Book the customer calls"
              : "Ideas and links collected over the week.\n\n- Read the onboarding study\n- Try the new export",
            snippet: item.meeting
              ? "We agreed to ship the beta on Friday."
              : "Ideas and links collected over the week.",
            category_id: categoryId,
            kind: item.meeting ? "meeting" : "manual",
            meeting_app: item.meeting ? "Zoom" : null,
            started_at: item.meeting ? item.date : null,
            ended_at: null,
            position:
              Math.min(
                0,
                ...[...state.notes.values()]
                  .filter((n) => n.category_id === categoryId)
                  .map((n) => n.position ?? 0),
              ) - 1,
            created_at: item.date,
            updated_at: item.date,
            deleted_at: null,
          };
          state.notes.set(note.id, note);
          created.get(run.id)?.push(note.id);
          sources.set(key, note.id);
          run.imported += 1;
        }
        progress({
          run_id: run.id,
          provider: run.provider,
          phase: "writing",
          done: i,
          total,
          current: item.title,
          run: null,
        });
        if (i % 3 === 0)
          ctx.emit({ type: "data_changed", scopes: ["notes", "categories"], note_id: null });
        timers.later(220, step);
      };
      timers.later(150, step);
      return { ...run };
    },

    importCancel(runId: string): null {
      const run = runs.find((r) => r.id === runId);
      if (run && run.status === "running") {
        run.status = "cancelled";
        run.finished_at = iso();
        progress({
          run_id: run.id,
          provider: run.provider,
          phase: "done",
          done: run.imported + run.skipped,
          total: run.imported + run.skipped,
          current: null,
          run: { ...run },
        });
      }
      return null;
    },

    importUndo(runId: string): number {
      const ids = created.get(runId);
      if (!ids?.length)
        throw appError(
          "not_found",
          "That import can no longer be undone. Select its notes in the sidebar and delete them instead.",
        );
      created.delete(runId);
      const run = runs.find((r) => r.id === runId);
      if (run?.status === "running") run.status = "cancelled";
      let moved = 0;
      for (const id of ids) {
        const note = state.notes.get(id);
        if (note && !note.deleted_at) {
          note.deleted_at = iso();
          moved += 1;
        }
      }
      ctx.emit({
        type: "data_changed",
        scopes: ["notes", "trash", "categories"],
        note_id: null,
      });
      return moved;
    },
  };
}

const SAMPLE_MEETINGS = [
  "Acme kickoff",
  "Weekly product sync",
  "Pricing review",
  "Design crit: onboarding",
  "1:1 with Priya",
  "Customer call: Northwind",
  "Roadmap planning",
  "Hiring debrief",
];

const SAMPLE_NOTES = [
  "Reading list",
  "Project Atlas",
  "Ideas for Q1",
  "Journal: Monday",
  "Recipes to try",
  "Interview questions",
  "Book notes: Shape Up",
  "Travel checklist",
];
