import type { AppError, BackendEvent, BackendEventOf } from "@/ipc/types";
import { createMockBackend, type MockBackend } from "./index";
import { makeExcerpt } from "./notes";

let backend: MockBackend;
let events: BackendEvent[];

beforeEach(() => {
  backend = createMockBackend({ latencyMs: 0 });
  events = [];
  backend.subscribe((e) => events.push(e));
});

afterEach(() => {
  backend.dispose();
  vi.useRealTimers();
});

const api = () => backend.commands;
const ofType = <T extends BackendEvent["type"]>(type: T) =>
  events.filter((e): e is BackendEventOf<T> => e.type === type);

async function rejection(promise: Promise<unknown>): Promise<AppError> {
  try {
    await promise;
  } catch (error) {
    return error as AppError;
  }
  throw new Error("expected the command to reject");
}

describe("seed", () => {
  it("loads demo data", async () => {
    const notes = await api().listNotes();
    expect(notes.length).toBeGreaterThanOrEqual(12);
    expect(notes.filter((n) => n.kind === "meeting")).toHaveLength(2);
    expect(await api().listCategories()).toHaveLength(4);
    expect(await api().listTrash()).toHaveLength(2);
    const transcript = await api().getTranscript("note_q3");
    expect(transcript.segments.length).toBeGreaterThan(5);
  });

  it("starts empty for a fresh install", async () => {
    const fresh = createMockBackend({ seed: "empty" });
    expect(await fresh.commands.listNotes()).toEqual([]);
    expect(await fresh.commands.listCategories()).toEqual([]);
    expect((await fresh.commands.getSettings()).onboarding_done).toBe(false);
  });

  it("returns copies, not live state", async () => {
    const note = await api().getNote("note_q3");
    note.title = "Changed";
    expect((await api().getNote("note_q3")).title).toBe("Q3 planning sync");
  });
});

describe("notes", () => {
  it("creates a note at the top of its bucket", async () => {
    const note = await api().createNote(null);
    expect(note.title).toBe("");
    const first = (await api().listNotes())[0];
    expect(first?.id).toBe(note.id);
  });

  it("discards a note only while nothing is in it", async () => {
    const blank = await api().createNote(null);
    const titled = await api().createNote(null);
    await api().renameNote(titled.id, "Plan");
    expect(await api().discardEmptyNote(titled.id)).toBe(false);
    expect(await api().discardEmptyNote(blank.id)).toBe(true);
    expect(await api().discardEmptyNote(blank.id)).toBe(false);
    const ids = (await api().listNotes()).map((n) => n.id);
    expect(ids).toContain(titled.id);
    expect(ids).not.toContain(blank.id);
  });

  it("renames and updates the body, snippet and versions", async () => {
    const note = await api().createNote("cat_product");
    await api().renameNote(note.id, "  Launch plan  ");
    const first = await api().updateNoteBody(note.id, "# Launch plan\n\nShip **on Tuesday**.\n");
    expect(first.title).toBe("Launch plan");
    expect(first.snippet).toBe("Ship on Tuesday.");
    // The first body has nothing before it to snapshot.
    expect(await api().listNoteVersions(note.id)).toHaveLength(0);
    await api().updateNoteBody(note.id, "# Launch plan\n\nShip on Wednesday.\n");
    const versions = await api().listNoteVersions(note.id);
    expect(versions).toHaveLength(1);
    expect(versions[0]?.body_md).toContain("Tuesday");
    expect(versions[0]?.author).toBe("user");
    // Within a minute of the last snapshot, no new version.
    await api().updateNoteBody(note.id, "# Launch plan\n\nShip on Thursday.\n");
    expect(await api().listNoteVersions(note.id)).toHaveLength(1);
  });

  it("moves notes between categories with fractional positions", async () => {
    await api().moveNote("note_inbox", "cat_hiring", 1.5);
    const hiring = (await api().listNotes()).filter((n) => n.category_id === "cat_hiring");
    expect(hiring.map((n) => n.id)).toEqual(["note_design_lead", "note_inbox", "note_loop"]);
    await api().moveNote("note_loop", "cat_hiring", null);
    const top = (await api().listNotes()).filter((n) => n.category_id === "cat_hiring")[0];
    expect(top?.id).toBe("note_loop");
    await api().moveNote("note_loop", null, null);
    expect((await api().listNotes())[0]?.id).toBe("note_loop");
  });

  it("sorts by category position, uncategorised first", async () => {
    const notes = await api().listNotes();
    expect(notes[0]?.category_id).toBeNull();
    await api().reorderCategory("cat_personal", 0.5);
    const after = (await api().listNotes()).filter((n) => n.category_id !== null);
    expect(after[0]?.category_id).toBe("cat_personal");
  });

  it("trashes, restores and empties Trash", async () => {
    await api().trashNote("note_inbox");
    expect((await api().listNotes()).some((n) => n.id === "note_inbox")).toBe(false);
    const trash = await api().listTrash();
    expect(trash[0]?.id).toBe("note_inbox");
    await api().restoreNote("note_inbox");
    expect((await api().listNotes()).some((n) => n.id === "note_inbox")).toBe(true);
    expect(await api().emptyTrash()).toBe(2);
    expect(await api().listTrash()).toEqual([]);
    await api().trashNote("note_reading");
    await api().deleteNoteForever("note_reading");
    expect((await rejection(api().getNote("note_reading"))).code).toBe("not_found");
  });

  it("moves a deleted category's notes to uncategorised", async () => {
    await api().deleteCategory("cat_hiring");
    const notes = await api().listNotes();
    expect(notes.find((n) => n.id === "note_design_lead")?.category_id).toBeNull();
    expect((await api().listCategories()).some((c) => c.id === "cat_hiring")).toBe(false);
  });

  it("validates categories", async () => {
    const error = await rejection(api().createCategory("   "));
    expect(error).toEqual({ code: "invalid", message: "Give the category a name." });
    expect((await rejection(api().renameCategory("nope", "x"))).code).toBe("not_found");
    const created = await api().createCategory("Research");
    const all = await api().listCategories();
    expect(all[all.length - 1]?.id).toBe(created.id);
  });

  it("rejects unknown notes with not_found", async () => {
    const error = await rejection(api().getNote("missing"));
    expect(error.code).toBe("not_found");
    expect(error.message).toMatch(/no longer exists/);
  });

  it("restores a version after snapshotting the current body", async () => {
    const before = (await api().listNoteVersions("note_onboarding")).length;
    const target = (await api().listNoteVersions("note_onboarding")).at(-1);
    if (!target) throw new Error("seed has versions");
    const current = (await api().getNote("note_onboarding")).body_md;
    const restored = await api().restoreNoteVersion(target.id);
    expect(restored.body_md).toBe(target.body_md);
    const versions = await api().listNoteVersions("note_onboarding");
    expect(versions).toHaveLength(before + 1);
    expect(versions[0]?.body_md).toBe(current);
  });
});

describe("search", () => {
  it("requires every term", async () => {
    const hits = await api().searchNotes("email verification", 20);
    expect(hits.length).toBeGreaterThan(0);
    for (const hit of hits) {
      const text = hit.excerpt.toLowerCase();
      expect(text.includes("email") || text.includes("verification")).toBe(true);
    }
    expect(await api().searchNotes("verification lisbon", 20)).toEqual([]);
    expect(await api().searchNotes("   ", 20)).toEqual([]);
  });

  it("ranks title hits first and marks matches", async () => {
    const hits = await api().searchNotes("pricing", 20);
    expect(hits[0]?.source).toBe("title");
    expect(hits[0]?.note.id).toBe("note_pricing");
    expect(hits.some((h) => h.source === "body")).toBe(true);
    expect(hits[0]?.excerpt).toMatch(/<mark>pricing<\/mark>/i);
  });

  it("finds transcript text with a timecode", async () => {
    const hits = await api().searchNotes("waitlist", 20);
    const transcript = hits.find((h) => h.source === "transcript");
    expect(transcript?.note.id).toBe("note_q3");
    expect(transcript?.at_ms).toBe(889_000);
  });

  it("excludes trashed notes and honours the limit", async () => {
    expect(await api().searchNotes("superseded", 20)).toEqual([]);
    expect(await api().searchNotes("the", 2)).toHaveLength(2);
  });

  it("escapes HTML and adds ellipses", () => {
    const text = `${"word ".repeat(20)}<b>pricing</b> & more ${"tail ".repeat(20)}`;
    const excerpt = makeExcerpt(text, ["pricing"]);
    expect(excerpt.startsWith("…")).toBe(true);
    expect(excerpt.endsWith("…")).toBe(true);
    expect(excerpt).toContain("&lt;b&gt;<mark>pricing</mark>&lt;/b&gt; &amp; more");
  });
});

describe("settings", () => {
  it("deep-merges sections and emits settings_changed", async () => {
    const next = await api().updateSettings({
      appearance: { theme: "dark" },
      keybinds: { "note.new": "Mod+Alt+N" },
    });
    expect(next.appearance).toEqual({
      theme: "dark",
      transparency: true,
      note_font: "sans",
      margin_placement: "side_panel",
      home_face: "random",
    });
    expect(next.recording.auto_detect).toBe(true);
    expect(next.keybinds).toEqual({ "note.new": "Mod+Alt+N" });
    const changed = ofType("settings_changed");
    expect(changed).toHaveLength(1);
    expect(changed[0]?.settings.appearance.theme).toBe("dark");
    // Replacing keybinds clears overrides.
    expect((await api().updateSettings({ keybinds: {} })).keybinds).toEqual({});
  });

  it("does not emit data_changed for UI writes", async () => {
    await api().createNote(null);
    await api().createCategory("Ideas");
    expect(ofType("data_changed")).toEqual([]);
  });
});

describe("recording", () => {
  it("records, transcribes and streams the summary", async () => {
    vi.useFakeTimers();
    const started = await api().startRecording({
      note_id: null,
      meeting_app: "Zoom",
      trigger: "manual",
    });
    expect(started.phase).toBe("starting");
    const noteId = started.note_id;
    if (!noteId) throw new Error("expected a note");
    expect((await api().getNote(noteId)).title).toMatch(/^Zoom meeting · \d{1,2}:\d{2}$/);
    expect(
      (
        await rejection(
          api().startRecording({ note_id: null, meeting_app: null, trigger: "hotkey" }),
        )
      ).code,
    ).toBe("conflict");

    await vi.advanceTimersByTimeAsync(8000);
    expect(ofType("recording_state_changed").map((e) => e.state.phase)).toEqual([
      "starting",
      "recording",
    ]);
    expect(ofType("audio_levels").length).toBeGreaterThan(100);
    const segments = ofType("transcript_segment");
    expect(segments.some((e) => !e.segment.is_final)).toBe(true);
    expect(segments.some((e) => e.segment.is_final && e.segment.channel === "mic")).toBe(true);
    expect((await api().getTranscript(noteId)).segments.length).toBeGreaterThan(0);
    // A recording the UI started gets no "Recording" toast (hotkey, tray and detection do).
    expect(ofType("toast").some((e) => e.toast.action?.type === "stop_recording")).toBe(false);

    await api().updateNoteBody(noteId, "- remember the launch email\n");
    await api().stopRecording();
    await vi.advanceTimersByTimeAsync(20_000);

    expect(ofType("recording_state_changed").map((e) => e.state.phase)).toEqual([
      "starting",
      "recording",
      "stopping",
      "diarizing",
      "summarizing",
      "idle",
    ]);
    const stream = ofType("note_body_stream");
    expect(stream.length).toBeGreaterThan(10);
    const last = stream.at(-1);
    expect(last?.done).toBe(true);
    expect(stream.filter((e) => e.done)).toHaveLength(1);
    expect(last?.body_md).toContain("## Summary");
    expect(last?.body_md).toContain("remember the launch email");
    const note = await api().getNote(noteId);
    expect(note.body_md).toBe(last?.body_md);
    expect((await api().listNoteVersions(noteId)).some((v) => v.author === "ai")).toBe(true);
    expect(
      ofType("data_changed").some((e) => e.scopes.includes("transcript") && e.note_id === noteId),
    ).toBe(true);
    const ready = ofType("toast").at(-1);
    expect(ready?.toast.title).toBe("Notes ready");
    expect(ready?.toast.action).toEqual({ type: "open_note", note_id: noteId });
    expect((await api().getRecordingState()).phase).toBe("idle");
    const transcript = await api().getTranscript(noteId);
    expect(transcript.segments.every((s) => s.is_final)).toBe(true);
    expect(transcript.speakers.some((s) => s.label === "Person 1")).toBe(true);
  });

  it("refuses to stop when idle", async () => {
    expect((await rejection(api().stopRecording())).code).toBe("invalid");
  });
});

describe("assistant", () => {
  it("streams a turn with a tool row, text and an edit proposal", async () => {
    vi.useFakeTimers();
    const thread = await api().createThread();
    const turnId = await api().sendMessage(thread.id, "Make the action items clearer", {
      note_id: "note_q3",
      note_title: "Q3 planning sync",
      is_recording: false,
    });
    await vi.advanceTimersByTimeAsync(10_000);
    const assistant = ofType("assistant").map((e) => e.event);
    expect(assistant[0]?.type).toBe("turn_started");
    expect(assistant.filter((e) => e.type === "tool")).toHaveLength(2);
    const text = assistant.flatMap((e) => (e.type === "text_delta" ? [e.delta] : [])).join("");
    expect(text).toContain("Q3 planning sync");
    const proposalEvent = assistant.find((e) => e.type === "proposal");
    expect(assistant.at(-1)).toMatchObject({
      type: "turn_finished",
      turn_id: turnId,
      end: { type: "completed" },
    });
    const messages = await api().getThreadMessages(thread.id);
    expect(messages.map((m) => m.role)).toEqual(["user", "assistant"]);

    if (proposalEvent?.type !== "proposal") throw new Error("expected a proposal");
    const applied = await api().applyProposal(proposalEvent.proposal.id, null);
    expect(applied.status).toBe("applied");
    expect((await api().getNote("note_q3")).body_md).toContain("## Next steps");
  });
});

describe("tasks", () => {
  it("runs a task and reports back", async () => {
    vi.useFakeTimers();
    const run = await api().runTaskNow("task_notes_review");
    expect(run.status).toBe("running");
    await vi.advanceTimersByTimeAsync(2500);
    expect(ofType("task_run_finished")[0]?.run.status).toBe("succeeded");
    expect(ofType("toast")[0]?.toast.action).toEqual({
      type: "open_task_run",
      task_run_id: run.id,
    });
    const detail = await api().getTaskRun(run.id);
    expect(detail.proposals.map((p) => p.kind.type).sort()).toEqual([
      "create_note",
      "edit_note",
      "merge_notes",
      "trash_notes",
    ]);
    const review = (await api().listTasks()).find((t) => t.id === "task_notes_review");
    expect(review?.pending_count).toBe(8);
    expect((await rejection(api().deleteTask("task_notes_review"))).code).toBe("invalid");
  });
});
