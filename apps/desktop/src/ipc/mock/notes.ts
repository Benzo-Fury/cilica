/** Notes, categories, versions, Trash, search, settings and app info. */
import type {
  AppInfo,
  Author,
  Category,
  Note,
  NoteSummary,
  NoteVersion,
  SearchHit,
  SearchSource,
  Settings,
  SettingsInput,
} from "@/ipc/types";
import { relinkAfterRename } from "./links";
import type { MockContext, MockState } from "./state";
import {
  appError,
  escapeHtml,
  iso,
  newId,
  notFound,
  snippetFrom,
  stripMarkdown,
  time,
} from "./util";

// ───────── Shared helpers (also used by recording, assistant and tasks) ─────────

export function summaryOf(note: Note): NoteSummary {
  const { body_md: _body, started_at: _s, ended_at: _e, ...rest } = note;
  return rest;
}

export function requireNote(state: MockState, noteId: string): Note {
  const note = state.notes.get(noteId);
  if (!note) throw notFound("note");
  return note;
}

export function requireLiveNote(state: MockState, noteId: string): Note {
  const note = requireNote(state, noteId);
  if (note.deleted_at) {
    throw appError("invalid", "That note is in Trash. Restore it first, then try again.");
  }
  return note;
}

function requireCategory(state: MockState, categoryId: string): Category {
  const category = state.categories.get(categoryId);
  if (!category) throw notFound("category");
  return category;
}

function liveNotes(state: MockState): Note[] {
  return [...state.notes.values()].filter((n) => !n.deleted_at);
}

function bucket(state: MockState, categoryId: string | null, exceptId?: string): Note[] {
  return liveNotes(state).filter((n) => n.category_id === categoryId && n.id !== exceptId);
}

function topPosition(state: MockState, categoryId: string | null, exceptId?: string): number {
  const positions = bucket(state, categoryId, exceptId)
    .map((n) => n.position)
    .filter((p): p is number => p !== null);
  return positions.length ? Math.min(...positions) - 1 : 1;
}

function endPosition(positions: (number | null)[]): number {
  const known = positions.filter((p): p is number => p !== null);
  return known.length ? Math.max(...known) + 1 : 1;
}

/** Snapshot a note's body as a version. */
export function snapshot(
  state: MockState,
  note: Note,
  author: Author,
  aiActionId: string | null = null,
): NoteVersion {
  const version: NoteVersion = {
    id: newId("ver"),
    note_id: note.id,
    title: note.title,
    body_md: note.body_md,
    author,
    ai_action_id: aiActionId,
    created_at: iso(),
  };
  state.versions.push(version);
  return version;
}

/** Write a body from the backend side (recording, assistant): versions before and after. */
export function writeBody(
  state: MockState,
  note: Note,
  bodyMd: string,
  aiActionId: string | null,
): void {
  if (note.body_md.trim() && note.body_md !== bodyMd) snapshot(state, note, "user");
  note.body_md = bodyMd;
  note.updated_at = iso();
  if (note.kind !== "meeting" || !note.snippet) note.snippet = snippetFrom(bodyMd);
  snapshot(state, note, "ai", aiActionId);
}

function compareNotes(state: MockState): (a: Note, b: Note) => number {
  const catRank = (id: string | null) => {
    if (id === null) return Number.NEGATIVE_INFINITY;
    return state.categories.get(id)?.position ?? Number.POSITIVE_INFINITY;
  };
  return (a, b) => {
    const ca = catRank(a.category_id);
    const cb = catRank(b.category_id);
    if (ca !== cb) return ca - cb;
    if (a.category_id !== b.category_id)
      return String(a.category_id).localeCompare(String(b.category_id));
    const pa = a.position ?? Number.POSITIVE_INFINITY;
    const pb = b.position ?? Number.POSITIVE_INFINITY;
    if (pa !== pb) return pa - pb;
    return b.updated_at.localeCompare(a.updated_at);
  };
}

// ───────── Commands ─────────

export function notesCommands(ctx: MockContext) {
  const { state } = ctx;
  return {
    listNotes(): NoteSummary[] {
      return liveNotes(state).sort(compareNotes(state)).map(summaryOf);
    },

    listTrash(): NoteSummary[] {
      return [...state.notes.values()]
        .filter((n) => n.deleted_at)
        .sort((a, b) => String(b.deleted_at).localeCompare(String(a.deleted_at)))
        .map(summaryOf);
    },

    getNote(noteId: string): Note {
      return requireNote(state, noteId);
    },

    createNote(categoryId: string | null): Note {
      if (categoryId !== null) requireCategory(state, categoryId);
      const now = iso();
      const note: Note = {
        id: newId("note"),
        title: "",
        body_md: "",
        snippet: "",
        category_id: categoryId,
        kind: "manual",
        meeting_app: null,
        started_at: null,
        ended_at: null,
        position: topPosition(state, categoryId),
        created_at: now,
        updated_at: now,
        deleted_at: null,
      };
      state.notes.set(note.id, note);
      return note;
    },

    renameNote(noteId: string, title: string): NoteSummary {
      const note = requireLiveNote(state, noteId);
      const before = note.title;
      note.title = title.trim();
      note.updated_at = iso();
      // Links to the old title follow the note, as in Obsidian.
      if (relinkAfterRename(state, noteId, before, note.title) > 0) {
        ctx.emit({ type: "data_changed", scopes: ["notes"], note_id: null });
      }
      return summaryOf(note);
    },

    updateNoteBody(noteId: string, bodyMd: string): NoteSummary {
      const note = requireLiveNote(state, noteId);
      if (note.body_md === bodyMd) return summaryOf(note);
      const last = state.versions
        .filter((v) => v.note_id === noteId)
        .reduce<string | null>((acc, v) => (acc && acc > v.created_at ? acc : v.created_at), null);
      const stale = !last || Date.now() - Date.parse(last) > time.MINUTE;
      if (stale && note.body_md.trim()) snapshot(state, note, "user");
      note.body_md = bodyMd;
      note.updated_at = iso();
      if (note.kind !== "meeting" || !note.snippet) note.snippet = snippetFrom(bodyMd);
      return summaryOf(note);
    },

    moveNote(noteId: string, categoryId: string | null, position: number | null): NoteSummary {
      const note = requireLiveNote(state, noteId);
      if (categoryId !== null) requireCategory(state, categoryId);
      note.position = position ?? topPosition(state, categoryId, noteId);
      note.category_id = categoryId;
      note.updated_at = iso();
      return summaryOf(note);
    },

    trashNote(noteId: string): null {
      const note = requireNote(state, noteId);
      note.deleted_at = iso();
      return null;
    },

    restoreNote(noteId: string): NoteSummary {
      const note = requireNote(state, noteId);
      if (note.category_id && !state.categories.has(note.category_id)) note.category_id = null;
      note.deleted_at = null;
      note.position = topPosition(state, note.category_id, note.id);
      return summaryOf(note);
    },

    deleteNoteForever(noteId: string): null {
      requireNote(state, noteId);
      removeNote(state, noteId);
      return null;
    },

    // Several at once: ids that no longer exist are skipped, as in the store.

    trashNotes(noteIds: string[]): number {
      let moved = 0;
      for (const id of noteIds) {
        const note = state.notes.get(id);
        if (!note || note.deleted_at) continue;
        note.deleted_at = iso();
        moved++;
      }
      return moved;
    },

    restoreNotes(noteIds: string[]): NoteSummary[] {
      const restored: NoteSummary[] = [];
      for (const id of [...noteIds].reverse()) {
        const note = state.notes.get(id);
        if (!note?.deleted_at) continue;
        if (note.category_id && !state.categories.has(note.category_id)) note.category_id = null;
        note.deleted_at = null;
        note.position = topPosition(state, note.category_id, note.id);
        restored.push(summaryOf(note));
      }
      return restored.reverse();
    },

    deleteNotesForever(noteIds: string[]): number {
      let removed = 0;
      for (const id of noteIds) {
        if (!state.notes.get(id)?.deleted_at) continue;
        removeNote(state, id);
        removed++;
      }
      return removed;
    },

    moveNotes(
      noteIds: string[],
      categoryId: string | null,
      position: number | null,
    ): NoteSummary[] {
      if (categoryId !== null) requireCategory(state, categoryId);
      if (position !== null && !Number.isFinite(position)) {
        throw appError("invalid", "That position is not a number. Try the move again.");
      }
      const moving = [...new Set(noteIds)].filter((id) => {
        const note = state.notes.get(id);
        return note && !note.deleted_at;
      });
      const staying = bucket(state, categoryId)
        .filter((n) => !moving.includes(n.id))
        .map((n) => n.position)
        .filter((p): p is number => p !== null);
      const count = moving.length;
      let positions: number[];
      if (position === null) {
        const top = staying.length ? Math.ceil(Math.min(...staying)) : count;
        positions = moving.map((_, i) => top - count + i);
      } else {
        const above = staying.filter((p) => p > position);
        const step = above.length ? (Math.min(...above) - position) / count : 1;
        positions = moving.map((_, i) => position + step * i);
      }
      return moving.map((id, i) => {
        const note = state.notes.get(id) as Note;
        note.category_id = categoryId;
        note.position = positions[i] ?? 0;
        return summaryOf(note);
      });
    },

    discardEmptyNote(noteId: string): boolean {
      const note = state.notes.get(noteId);
      if (!note || note.deleted_at || note.kind !== "manual") return false;
      if (note.title.trim() || note.body_md.trim()) return false;
      removeNote(state, noteId);
      return true;
    },

    emptyTrash(): number {
      const trashed = [...state.notes.values()].filter((n) => n.deleted_at);
      for (const n of trashed) removeNote(state, n.id);
      return trashed.length;
    },

    // ── Categories ──

    listCategories(): Category[] {
      return [...state.categories.values()].sort(
        (a, b) =>
          (a.position ?? Number.POSITIVE_INFINITY) - (b.position ?? Number.POSITIVE_INFINITY),
      );
    },

    createCategory(name: string): Category {
      const trimmed = name.trim();
      if (!trimmed) throw appError("invalid", "Give the category a name.");
      const category: Category = {
        id: newId("cat"),
        name: trimmed,
        position: endPosition([...state.categories.values()].map((c) => c.position)),
        collapsed: false,
      };
      state.categories.set(category.id, category);
      return category;
    },

    renameCategory(categoryId: string, name: string): Category {
      const category = requireCategory(state, categoryId);
      const trimmed = name.trim();
      if (!trimmed) throw appError("invalid", "Give the category a name.");
      category.name = trimmed;
      return category;
    },

    reorderCategory(categoryId: string, position: number | null): Category {
      const category = requireCategory(state, categoryId);
      category.position =
        position ??
        endPosition(
          [...state.categories.values()].filter((c) => c.id !== categoryId).map((c) => c.position),
        );
      return category;
    },

    setCategoryCollapsed(categoryId: string, collapsed: boolean): Category {
      const category = requireCategory(state, categoryId);
      category.collapsed = collapsed;
      return category;
    },

    deleteCategory(categoryId: string): null {
      requireCategory(state, categoryId);
      let next = endPosition(bucket(state, null).map((n) => n.position));
      const moved = [...state.notes.values()]
        .filter((n) => n.category_id === categoryId)
        .sort((a, b) => (a.position ?? 0) - (b.position ?? 0));
      for (const n of moved) {
        n.category_id = null;
        if (!n.deleted_at) {
          n.position = next;
          next += 1;
        }
      }
      state.categories.delete(categoryId);
      return null;
    },

    // ── Search ──

    searchNotes(query: string, limit: number): SearchHit[] {
      return search(state, query, limit);
    },

    // ── Versions ──

    listNoteVersions(noteId: string): NoteVersion[] {
      requireNote(state, noteId);
      return state.versions
        .filter((v) => v.note_id === noteId)
        .sort((a, b) => b.created_at.localeCompare(a.created_at));
    },

    restoreNoteVersion(versionId: string): Note {
      const version = state.versions.find((v) => v.id === versionId);
      if (!version) throw notFound("version");
      const note = requireLiveNote(state, version.note_id);
      snapshot(state, note, "user");
      note.body_md = version.body_md;
      note.updated_at = iso();
      if (note.kind !== "meeting" || !note.snippet) note.snippet = snippetFrom(note.body_md);
      return note;
    },

    exportNoteMarkdown(noteId: string, path: string): null {
      const note = requireNote(state, noteId);
      console.info(`[mock] Exported "${note.title || "Untitled"}" to ${path}`);
      return null;
    },
  };
}

function removeNote(state: MockState, noteId: string): void {
  state.notes.delete(noteId);
  state.versions = state.versions.filter((v) => v.note_id !== noteId);
  state.transcripts.delete(noteId);
}

// ───────── Search ─────────

const SOURCE_RANK: Record<SearchSource, number> = { title: 0, body: 1, transcript: 2 };
const EXCERPT_LENGTH = 90;

function search(state: MockState, query: string, limit: number): SearchHit[] {
  const terms = query
    .toLowerCase()
    .split(/\s+/)
    .map((t) => t.trim())
    .filter(Boolean);
  const first = terms[0];
  if (!first || limit <= 0) return [];

  const hits: (SearchHit & { rank: number })[] = [];
  for (const note of liveNotes(state)) {
    const title = note.title;
    const body = stripMarkdown(note.body_md).replace(/\n/g, " ");
    const segments = state.transcripts.get(note.id)?.segments ?? [];
    const transcriptText = segments.map((s) => s.text).join(" ");
    const haystack = `${title}\n${body}\n${transcriptText}`.toLowerCase();
    if (!terms.every((t) => haystack.includes(t))) continue;

    let source: SearchSource;
    let excerptSource: string;
    let atMs: number | null = null;
    if (title.toLowerCase().includes(first)) {
      source = "title";
      excerptSource = body || title;
    } else if (body.toLowerCase().includes(first)) {
      source = "body";
      excerptSource = body;
    } else {
      source = "transcript";
      const segment = segments.find((s) => s.text.toLowerCase().includes(first));
      excerptSource = segment?.text ?? transcriptText;
      atMs = segment?.start_ms ?? null;
    }
    hits.push({
      note: summaryOf(note),
      source,
      excerpt: makeExcerpt(excerptSource, terms),
      at_ms: atMs,
      rank: SOURCE_RANK[source],
    });
  }
  hits.sort((a, b) => a.rank - b.rank || b.note.updated_at.localeCompare(a.note.updated_at));
  return hits.slice(0, limit).map(({ rank: _rank, ...hit }) => hit);
}

/** About 90 characters around the first match, matches wrapped in <mark>, the rest escaped. */
export function makeExcerpt(text: string, terms: string[]): string {
  const lower = text.toLowerCase();
  const firstIndex = terms.map((t) => lower.indexOf(t)).find((i) => i >= 0) ?? 0;
  let start = Math.max(0, firstIndex - 30);
  let end = Math.min(text.length, start + EXCERPT_LENGTH);
  if (end - start < EXCERPT_LENGTH) start = Math.max(0, end - EXCERPT_LENGTH);
  // Snap to word boundaries so we never cut a word in half.
  if (start > 0) {
    const space = text.indexOf(" ", start);
    if (space !== -1 && space < firstIndex) start = space + 1;
  }
  if (end < text.length) {
    const space = text.lastIndexOf(" ", end);
    if (space > firstIndex) end = space;
  }
  const slice = text.slice(start, end).trim();
  const pattern = new RegExp(
    `(${terms.map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})`,
    "gi",
  );
  const marked = slice
    .split(pattern)
    .map((part, i) => (i % 2 === 1 ? `<mark>${escapeHtml(part)}</mark>` : escapeHtml(part)))
    .join("");
  return `${start > 0 ? "…" : ""}${marked}${end < text.length ? "…" : ""}`;
}

// ───────── Settings and app info ─────────

/** The welcome note the shell writes when setup finishes (crates/store/src/welcome.rs). */
const WELCOME_BODY = [
  "Cilica is a notes app with an AI assistant and a meeting summariser. This note covers the basics. Delete it whenever you like.",
  "## Write",
  "Notes are plain Markdown. Type `#` for a heading, `-` for a list and `[[` to link to another note. Shortcuts can be changed in Settings › Keybinds.",
  "## Ask Cilica",
  "Press **⌘J** to open the assistant. Ask it to sort notes into categories, summarise a meeting or tidy up this note. Every change can be undone.",
  "## Record a meeting",
  "Press **⌘⇧R** from anywhere, or click Record in the sidebar. Cilica hears your microphone and the call, transcribes it on this computer, and writes a summary with decisions and action items when you stop.",
  "## Find anything",
  "Press **⌘K** to search every note and transcript, or to ask Cilica a question about them.",
].join("\n\n");

export function settingsCommands({ state, emit }: MockContext) {
  return {
    getSettings(): Settings {
      return state.settings;
    },

    updateSettings(input: SettingsInput): Settings {
      state.settings = mergeSettings(state.settings, input);
      emit({ type: "settings_changed", settings: JSON.parse(JSON.stringify(state.settings)) });
      return state.settings;
    },

    getAppInfo(): AppInfo {
      const ua = typeof navigator === "undefined" ? "Mac" : navigator.userAgent;
      const os = /Mac/i.test(ua) ? "macos" : /Win/i.test(ua) ? "windows" : "linux";
      return {
        version: "0.1.0-mock",
        os,
        arch: "aarch64",
        has_window_material: false,
        data_dir:
          os === "macos"
            ? "/Users/you/Library/Application Support/app.cilica.desktop"
            : os === "windows"
              ? "C:\\Users\\you\\AppData\\Roaming\\app.cilica.desktop"
              : "/home/you/.local/share/app.cilica.desktop",
      };
    },

    showMainWindow(): null {
      return null;
    },

    showOnboardingWindow(): null {
      return null;
    },

    /** As the shell does: mark setup done, write the welcome note once, open it in the main window. */
    finishOnboarding(): null {
      if (state.settings.onboarding_done) return null;
      state.settings = { ...state.settings, onboarding_done: true };
      emit({ type: "settings_changed", settings: JSON.parse(JSON.stringify(state.settings)) });
      const now = iso();
      const note: Note = {
        id: newId("note"),
        title: "Welcome to Cilica",
        body_md: WELCOME_BODY,
        snippet: "Cilica is a notes app with an AI assistant and a meeting summariser.",
        category_id: null,
        kind: "manual",
        meeting_app: null,
        started_at: null,
        ended_at: null,
        position: topPosition(state, null),
        created_at: now,
        updated_at: now,
        deleted_at: null,
      };
      state.notes.set(note.id, note);
      emit({ type: "data_changed", scopes: ["notes"], note_id: null });
      emit({ type: "open_note", note_id: note.id });
      return null;
    },

    /** In the browser preview, setup opens in a new tab with a fresh mock. */
    openOnboarding(): null {
      if (typeof window !== "undefined") window.open("/?window=onboarding&fresh", "_blank");
      return null;
    },
  };
}

/** Deep-merge each settings section. `keybinds`, when given, replaces the whole map. */
export function mergeSettings(current: Settings, input: SettingsInput): Settings {
  const pick = <T extends object>(base: T, patch: Partial<T> | undefined): T => {
    if (!patch) return base;
    const out = { ...base };
    for (const [key, value] of Object.entries(patch)) {
      if (value !== undefined) (out as Record<string, unknown>)[key] = value;
    }
    return out;
  };
  return {
    // An old kept "word" face reads as Verbal tics, as serde's alias does in Rust.
    appearance: pick(
      current.appearance,
      input.appearance && {
        ...input.appearance,
        home_face: input.appearance.home_face === "word" ? "tics" : input.appearance.home_face,
      },
    ),
    recording: pick(current.recording, input.recording),
    transcription: pick(
      current.transcription,
      input.transcription && {
        ...input.transcription,
        cloud:
          input.transcription.cloud && pick(current.transcription.cloud, input.transcription.cloud),
      },
    ),
    ai: pick(current.ai, input.ai),
    sounds: pick(current.sounds, input.sounds),
    swear_jar: pick(current.swear_jar, input.swear_jar),
    writing: pick(current.writing, input.writing),
    keybinds: input.keybinds ? { ...input.keybinds } : { ...current.keybinds },
    launch_at_login: input.launch_at_login ?? current.launch_at_login,
    onboarding_done: input.onboarding_done ?? current.onboarding_done,
    updates: pick(current.updates, input.updates),
  };
}
