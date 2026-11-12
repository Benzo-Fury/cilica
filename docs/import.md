# Importing notes from other apps

> v0.2, 12 Nov 2026. Built: see Status below.  Facts about other products were researched on 12 Nov 2026; anything marked **unverified** needs a real sample file or account before we write its parser.

Switching to Margin should take one click and a minute. The user should never have to know what a "VTT" or a "GraphQL key" is. This page lists every competitor we know of, says how each one's notes can get out, and specifies one import pipeline that handles all of them.

## Status

Built on 12 Nov 2026 (`crates/import`, Settings › Import).

- **The picker covers 45 apps.** Every app in the tables below that has a route is in Settings › Import's searchable picker, with its steps and with lists of what comes across and what doesn't.
- **On-this-computer reading:**
  - Obsidian vaults, found through Obsidian's own `obsidian.json`.
  - Anarlog and Hyprnote, read from their database. Checked against a real install: meetings, summaries, notes, named speakers and timed transcripts.
  - Meetily, read from its database.
  - Apple Notes, through `osascript`.
- **API keys:** Granola, Fireflies, Fathom, Otter (Enterprise), tl;dv, Circleback, MeetGeek, Jamie, Limitless. Each was built against the vendor's published API reference. **None has been tried with a real account yet.**
- **Files:** every other app, through the shared parsers. Any app can also take pasted text.
- **Differences from this proposal:**
  - The schema uses `notes.transcript_timed` (0/1) rather than a three-way `transcript_timing`.
  - Audio import, the generic MCP client, OAuth connectors (Read AI, Notion API, Zoom, Graph, Google) and continuous sync are not built.
  - Granola's CSV columns are matched by name (title, summary, transcript…), because no sample was available.

## Principles

1. **Detect, don't ask.** On the Import screen, show sources we found on this machine first ("Anarlog is on this Mac: 212 meetings"). If the user drops a file, work out what it is from the file itself.
2. **Show a preview before anything is written.** The preview gives the number of notes, the date range and what will come across (summary, your notes, transcript, speakers, timestamps), what won't, and any duplicates.
3. **Never lose data, never duplicate it.** Importing again only brings in what is new. A whole import can be undone in one step: it goes to Trash and can be restored.
4. **Stay local-first.** Reading files and local databases never touches the network. An account connection talks only to that vendor's own host. Margin makes no LLM calls during an import.
5. **Work in the background.** An import can be cancelled and resumed, and it reports progress through the toast and event system. The user can keep working meanwhile.
6. **Treat imported content as data.** It is untrusted text and gets the same prompt-injection isolation as transcripts.

## Who we import from

Competitors fall into four groups. The **route** is how Margin gets the data out, listed best first. **Fidelity** is what that route carries: S = AI summary, N = the user's own typed notes, T = transcript, Sp = speaker names, Ts = timestamps.

### 1. Bot-free desktop note takers (direct competitors)

These users are the most likely to switch, so they come first.

| Product | Route (best first) | Fidelity | Phase |
| --- | --- | --- | --- |
| **Granola** | (a) Personal API key, Business plan and up: `public-api.granola.ai/v1/notes?include=transcript`. (b) MCP with OAuth sign-in: full history on paid plans; on Free only the last 30 days and no transcripts. (c) Settings → Generate CSV: free, all history, one export a day, notes without a summary are left out. | API: S N T Sp(me/them) Ts. CSV: S T, other fields unverified | 1 |
| **Anarlog** (formerly Hyprnote / Char) | (a) Read `app.db` read-only (verified on this Mac, see the Anarlog adapter below). (b) `anarlog meetings export <id> --format json` CLI. (c) Markdown folder. | S N T Sp Ts, audio | 1 |
| **Meetily** | Read `meeting_minutes.sqlite` read-only. | S N T Sp(mic/system) Ts | 2 |
| **MacWhisper** | (a) Read `Database/main.sqlite` (unofficial schema). (b) SRT, VTT or Markdown export. | T Sp(partial) Ts | 2 |
| **Superwhisper** | `superwhisper meetings --json` + `meetings read <id> --transcript`. | S N T, Sp and Ts unverified | 2 |
| **Shadow** | Markdown folder: Shadow already saves meetings as `.md` files. | S N T, layout unverified | 2 |
| **Quill Meetings** | Read `~/Library/Application Support/Quill/quill.db` (unofficial). | S T Sp | 3 |
| **Jamie** | Personal API key, Pro plan and up: `beta-api.meetjamie.ai/v1/me/meetings.get`. MCP with OAuth as a fallback. | S N T Sp, **no timestamps** | 2 |
| **Krisp** | MCP with OAuth (`mcp.krisp.ai`). Per-meeting TXT. | S T, Sp and Ts unverified | 3 |
| **Bluedot** | MCP with OAuth. TXT or PDF export. | S T Sp Ts | 3 |
| **Notion AI Meeting Notes** | Notion OAuth: `POST /v1/blocks/meeting_notes/query`, then `GET /v1/pages/{id}/markdown?include_transcript=true`. | S N T, Sp and Ts unverified | 2 |
| **Plaud** (NotePin, Desktop) | Official CLI (`plaud transcript`, `plaud summary`) or MCP. SRT + MD export. | S T Sp Ts | 3 |
| **Apple Notes recordings / Voice Memos** | Notes: the user exports Markdown (macOS 26+). Voice Memos: read the transcript JSON from the `tsrp` atom inside each `.m4a`, or re-transcribe the audio with Margin's local Whisper. | Notes: S N T. Memos: T | 3 |
| **Limitless / Rewind** (Meta) | Lifelogs API key (`GET /v1/lifelogs?includeContents`). Export zip. **The service may shut down around the end of 2026, so ship quickly if at all.** | S T Sp Ts, audio | 2 (time-boxed) |

### 2. Bot-based cloud meeting assistants

| Product | Route (best first) | Fidelity | Gating | Phase |
| --- | --- | --- | --- | --- |
| **Otter.ai** | (a) Bulk ZIP export (Business plan and up) or per-meeting TXT/DOCX/SRT files. (b) REST API key on Enterprise. (c) MCP with OAuth. | S T Sp Ts | Free plan: TXT only | 1 |
| **Fireflies.ai** | (a) GraphQL API with a personal key, on every plan: `transcripts{sentences{speaker_name,start_time,end_time,text}, summary, meeting_attendees, date}`. (b) Account takeout ZIP, any plan, emailed in 24–48 h. | S T Sp Ts, attendees, audio (takeout) | Free: 500 requests a day | 1 |
| **Fathom** (now Superhuman) | REST API key on every plan: `GET /external/v1/meetings?include_transcript&include_summary&include_action_items`. | S T Sp Ts, action items, invitees | Pace to 30 requests a minute | 1 |
| **tl;dv** | API key (`pasta.tldv.io/v1alpha1`): meetings, transcript, notes. | S N T Sp Ts | Pro plan and up. Free plan: paste only | 2 |
| **Read AI** | OAuth 2.1 with PKCE (no static keys): `api.read.ai/v1/meetings/{id}?expand[]=…`. TXT export as a fallback. | S T Sp Ts | All plans (beta) | 2 |
| **Circleback** | API key (`cb_…`), Free plan included: notes in Markdown, action items, attendees, transcript. | S T Sp Ts | Free: limited history | 2 |
| **Fellow** | API key on the workspace subdomain (an admin must enable it): notes, recordings, `speech_segments`. | S N T Sp Ts | Any paid plan | 2 |
| **Grain** | (a) Bulk Markdown ZIP (20 meetings per batch). (b) Personal token. | S T Sp Ts | Plan floor unclear | 2 |
| **MeetGeek** | API key, separate EU and US regions: meetings, transcript, summary, highlights. | S T Sp Ts | Basic plan and up | 2 |
| **Avoma** | (a) VTT per meeting. (b) Admin API key. | T Sp Ts (VTT); S via API | API: Organization plan | 3 |
| **Sembly** | Markdown export per meeting (notes + transcript). | S T Sp | Unverified | 2 (Markdown parser) |
| **Tactiq** | TXT export per meeting. MCP with OAuth. | T Sp | Plan-dependent | 2 (speaker-TXT parser) |
| **Supernormal** | API (Business plan, support must enable it). Zapier to Docs. | T, S unclear | Business | 3 |
| **Bubbles** | Personal token or OAuth. | S T Sp, Ts unclear | Paid | 3 |
| **Notta** | TXT, DOCX or SRT per file. | T Sp Ts | — | 2 (generic parsers) |
| Gong, Chorus | Admin API keys only, so an individual rarely has them. | T Sp Ts | Enterprise | Not planned |

### 3. Meeting platforms' own AI notes

| Product | Route | Fidelity | Phase |
| --- | --- | --- | --- |
| **Microsoft Teams** | (a) Transcript `.vtt` (`<v Name>` voice tags) or `.docx` that the user downloaded. (b) Microsoft Graph `onlineMeetings/{id}/transcripts/{id}/content?$format=text/vtt`, delegated sign-in, work accounts only, admin may block it. Copilot notes come from the `aiInsights` endpoint and need a Copilot licence. | T Sp Ts; S with Copilot | 1 (VTT), 3 (Graph) |
| **Zoom** | (a) Cloud-recording VTT or exported summary DOCX/MD. (b) User OAuth: `/meetings/{id}/meeting_summary` + `/meetings/{uuid}/transcript`. "My Notes" has no API. | S T Sp Ts | 1 (VTT), 3 (OAuth) |
| **Google Meet** (Gemini "Take notes for me") | (a) The Docs in Drive → Google Meet folder, downloaded as `.md` or `.docx`. (b) Google OAuth + Picker (`drive.file` scope, so no restricted-scope review) → `files.export` as `text/markdown`. The Meet API's transcript entries are deleted after 30 days. | S T Sp | 1 (files), 3 (Picker) |
| **Webex** | Downloaded meeting content (TXT/VTT + summary). Transcripts and Summaries APIs. | S T Sp Ts | 1 (VTT), 3 (API) |
| **Slack huddles** | `canvases.getContent` → Markdown. No export in the app's UI. | S, T inside the canvas | 3 |

### 4. General notes apps where people keep meeting notes

| Product | Route | What survives | Phase |
| --- | --- | --- | --- |
| **Notion** | "Markdown & CSV" zip. Strip the 32-hex ID suffix from every filename, but keep it for de-duplication and links. | Hierarchy. Dates and tags only from database CSVs | 1 |
| **Obsidian** (and Shadow, Reflect, Craft exports) | Pick a folder → Markdown + YAML frontmatter. Skip `.obsidian/`. Resolve `![[embed]]` by filename anywhere in the vault. | Folders, frontmatter, file times, attachments | 1 |
| **Apple Notes** | (a) macOS 26+ File → Export As → Markdown. (b) JXA script `Notes.app` → HTML, dates and folders (needs the Automation prompt). (c) Parse `NoteStore.sqlite` protobuf (highest fidelity, highest upkeep, needs Full Disk Access). | Dates and folders (JXA). Attachments only from NoteStore | 2 |
| **Evernote** | `.enex` per notebook: stream with `quick-xml`, convert ENML → Markdown, match `<en-media hash>` to the resource's MD5. The notebook name comes from the filename. | Created, updated, tags, attachments | 2 |
| **Bear** | `.bear2bk` = zip of `.textbundle` entries. Dates come from `info.json`; tags are written inline as `#tag`. | Dates, tags, assets | 2 |
| **Google Keep** | Takeout `Keep/*.json`: `textContent`/`listContent`, `createdTimestampUsec`, `labels`. | Dates, labels, attachments, archived/trashed | 3 |
| **Google Docs** | `.docx` or `.md` from Drive. | Content only | 1 (generic) |
| **Logseq** | Folder: flatten `- ` bullet blocks, take journal dates from filenames, read `key:: value` properties. | Dates, properties | 3 |
| **OneNote** | Graph `Notes.Read` → page HTML → Markdown. | Dates, notebook/section | 3 |
| **Mem** | Single-file JSON export. | Unverified | 3 |
| **Microsoft Loop** | No export route that works reliably. Accept pasted text. | — | Paste only |

### Routes this does not take

- **Granola's local cache.** Since July 2026 (app 7.427) it is `cache-v6.json.enc`, and the key sits in a keychain item that only Granola can read. Do not try to get around that.
- **Rewind's database.** It is encrypted with SQLCipher.
- **Screen-scraping web apps or driving a headless browser with the user's session.** It is fragile, it breaks the vendors' terms, and it would hold the user's credentials.
- **Secret columns in other apps' databases.** Meetily's `settings` and `transcript_settings` tables hold plaintext API keys. Adapters name the tables they read and nothing else.

## Four routes, one pipeline

Every adapter uses one of four routes. Each one turns its input into the same intermediate `ImportedNote`, and one writer saves those to the store.

| Route | What the user does | Used for |
| --- | --- | --- |
| **On this Mac** | Nothing. The card already says "Found Anarlog: 212 meetings". Clicking Import opens the preview. | Anarlog, Meetily, MacWhisper, Quill, Superwhisper/Plaud/Anarlog CLIs, Voice Memos |
| **Connect account** | Paste an API key (with a "Where do I find this?" link to the vendor's settings page), or sign in in the browser (OAuth with PKCE and a loopback redirect). | Granola, Fireflies, Fathom, Otter Enterprise, tl;dv, Circleback, Fellow, Grain, MeetGeek, Jamie, Limitless, Read AI, Notion, Teams/Zoom/Google later |
| **Drop a file or folder** | Drag onto the window, or choose a file. Margin recognises the format. | Every export: ZIP, CSV, VTT, SRT, TXT, MD, DOCX, ENEX, `.bear2bk`, Takeout |
| **Paste** | Paste a transcript or notes into the import sheet. | Copy-only cases (tl;dv Free, Jamie Free/Plus, Microsoft Loop, Granola's "Copy notes") |

A further route, **MCP with OAuth**, is deferred. One generic Streamable-HTTP MCP client with dynamic client registration could reach Granola, Otter, Krisp, Bluedot, Jamie, Read, Fellow, Avoma, Circleback, Grain, Tactiq and Bubbles. Each vendor names its tools differently, though, so each still needs a small mapping, and most of them already offer a better REST route. Add it in phase 3, where it covers Krisp and Bluedot, which have no REST API.

## User experience

### Where it lives

- **Onboarding.** After permissions, a step titled "Bring your notes" shows the detected sources. It can be skipped.
- **Settings → Import.** The full catalogue as a searchable grid of source cards. Detected sources come first, then by popularity.
- **Command palette.** "Import notes…", plus "Import from Granola" and the like for each source.
- **Drop anywhere.** Dropping a supported file onto the main window opens the import sheet already scanned.
- **Empty sidebar.** The empty state offers "Import from another app" next to New note.

### The flow

```
Pick source ─▶ Provide input ─▶ Preview ─▶ Import (background) ─▶ Done toast with Undo
   │              │               │
   │              │               ├─ 214 notes · Mar 2024 – Nov 2026
   │              │               ├─ Summary ✓  Your notes ✓  Transcript ✓  Speakers ✓  Timestamps –
   │              │               ├─ 12 already imported · 3 look like meetings you recorded in Margin
   │              │               ├─ Which of these is you?  [Sam Lee ▾]  (pre-filled from the most frequent "me" speaker)
   │              │               ├─ Folders: keep Granola's folders as categories  /  put everything in "From Granola"
   │              │               └─ Include audio (Anarlog, Fireflies takeout)  ☐
   │              └─ Detected: nothing. API key: one field. File: drop zone. OAuth: "Continue in browser".
   └─ Cards: name, the app's own icon (`features/import/logos/`, from its website, the App Store or macOS; each file is the whole mark, so nothing draws a tile or outline around it; `bun run import-apps` rebuilds the README's grid from them), "Found on this Mac" badge, one-line route hint
```

- **Card copy explains the route in plain words.** For example: "Paste your Fireflies API key. You'll find it in Fireflies → Settings → Developer settings."
- **The preview shows five sample notes** rendered as Margin would show them, so the user can see the result before committing.
- **Progress** appears as a `toast.promise`-style pill ("Importing from Granola · 84 of 214"). The import sheet can be closed. Cancelling keeps what has already been imported.
- **The done toast** reads "Imported 214 notes from Granola", with a Review action that filters the sidebar to that import and an Undo action.
- **Undo** sends every note from that import to Trash through the destructive confirm card ("Move 214 imported notes to Trash?").
- **Partial failures** go in a report: "211 imported, 3 skipped. Two had no content, and one could not be read; the file may be damaged." Each line names the source item.

### Copy

Sentence case and plain verbs, as in the design system. Errors say what went wrong and what to do next. For example:

- Bad API key: "Fireflies didn't accept that key. Copy it again from Fireflies → Settings → Developer settings."
- Plan limit: "Fathom is limiting how fast Margin can read. The import will finish on its own in about 6 minutes."
- Unreadable file: "Margin couldn't read this file as a transcript. Try exporting it again as .txt or .vtt."

Never write "AI". The assistant is "Margin".

## Mapping into Margin

### The intermediate type

Every adapter produces this type and nothing else. It lives in `crates/import`, not in `domain`, because it never crosses IPC.

```rust
pub struct ImportedNote {
    pub external_id: String,                 // stable id in the source; file path + hash for files
    pub title: String,
    pub created_at: Option<Timestamp>,
    pub updated_at: Option<Timestamp>,
    pub started_at: Option<Timestamp>,       // meeting times, if it was a meeting
    pub ended_at: Option<Timestamp>,
    pub meeting_app: Option<String>,         // "Zoom", "Google Meet"… when the source says so
    pub attendees: Vec<Attendee>,            // name, email?
    pub summary_md: Option<String>,          // the competitor's AI notes
    pub user_notes_md: Option<String>,       // what the user typed themself
    pub action_items: Vec<ActionItem>,       // only when separate from the summary
    pub transcript: Option<ImportedTranscript>,
    pub folder_path: Vec<String>,            // ["Clients", "Acme"]
    pub tags: Vec<String>,
    pub attachments: Vec<ImportedAttachment>,
    pub audio: Option<ImportedAudio>,        // path or URL + mime, fetched only if the user asked
}

pub struct ImportedTranscript {
    pub timing: TranscriptTiming,            // Exact | Approximate | None
    pub turns: Vec<Turn>,                    // speaker, start_ms?, end_ms?, text
}

pub enum SpeakerRef { Me, Them, Named(String), Unknown }
```

### Field mapping

| Margin | From |
| --- | --- |
| `notes.title` | `title`, or "{Source} meeting · {date}" if empty |
| `notes.body_md` | The summary as-is, then `## My notes` + the user's notes when both exist, then `## Action items` (`- [ ] text, Owner, due date`) when the action items were separate. When there are only notes, the body is just the notes. This matches Margin's own layout: summary first, the user's jottings below. |
| `notes.kind` | `meeting` if there is a transcript or `started_at`; otherwise `manual` |
| `notes.started_at` / `ended_at` / `meeting_app` | As given. `duration_ms` comes from the times, or from the last transcript turn |
| `notes.created_at` / `updated_at` | From the source, so that imported notes sort into history and don't all sit on top as "today" |
| `notes.category_id` | See Categories |
| `notes.snippet` | First sentence of the summary, or of the notes, computed locally. No LLM call |
| `speakers` | One row per distinct speaker. `Me` and the name the user picked as "you" get `is_self = 1` and `label = "You"`. Others get their name, or "Person N" if unnamed. Colours rotate 1–8. No embedding |
| `segments` | One row per turn. Channel: `mic` for self, `system` for everyone else. Mic/system sources (Granola, Meetily, Anarlog) map directly. Word-level sources (Anarlog `words_json`) are grouped into turns, split on a speaker or channel change or a gap over 1.5 s |
| Untimed transcripts (Jamie, Bubbles, pasted text) | `start_ms` = turn index × 1000 so order is kept, and the note is marked `transcript_timing = 'none'`, so the transcript panel hides timestamps rather than showing invented ones |
| Attachments | Copied under the app data dir; image embeds rewritten to the local path |
| Audio | Only if "Include audio" is on. Transcoded to Opus through `crates/audio`. Retention counts from the import date, not the meeting date, so old meetings aren't purged straight away |
| `note_versions` | One snapshot, author `user`, so History shows "Imported from Granola" as the first version |

### People

- **Speaker names are never auto-linked to Saved people.** Saved people are matched by voiceprint, and imported audio has none.
- **The preview lists distinct speaker names with counts.** An exact, case-insensitive name match to a Saved person is pre-ticked as a suggestion, and the user confirms it.
- **Optional: "Save frequent speakers as people".** Anyone in 3 or more imported meetings becomes a Saved person with `sample_count = 0`. A voiceprint gets added later, the first time they speak in a real Margin recording and the user confirms the "Is Person 2 Sam?" suggestion.

### Categories

Categories in Margin are flat.

- **Source has folders** (Granola folders, Notion pages, Obsidian directories, Evernote notebooks, Meetily folders): the default is to keep them. A nested path becomes "Parent / Child". An existing category with the same name, case-insensitive, is reused.
- **Source has no folders:** everything goes into one new category, "From {Source}". The user can switch this off so notes land uncategorised.
- **Tags** are written into the body as a final `Tags: #a #b` line. Margin has no tags yet, and dropping them would lose data.

### Duplicates and re-import

- **Same source, same item:** a `(source, external_id)` key.
  - If the item is unchanged (same content hash), skip it.
  - If it changed at the source but nobody edited it in Margin since import, update it and record a new version.
  - If it was edited in Margin since import, keep Margin's version and report "kept your edits".
- **Same meeting from two places** (recorded in Margin and in Granola, or imported from both Otter and Fireflies): it is a likely duplicate when the start time is within 5 minutes and the titles share at least 60% of their tokens. The preview groups these. The default is to import anyway and tag the pair, so the Notes review task can suggest a merge through the normal destructive-tier flow. An import never merges anything silently.

## Architecture

### New crate: `crates/import`

```
crates/import/
  src/
    lib.rs            Importer service: detect, scan, run, cancel, undo
    model.rs          ImportedNote, ImportedTranscript, Attendee, …
    writer.rs         ImportedNote → store (notes, speakers, segments, versions, links), in one transaction per note
    detect.rs         looks for known local paths and CLIs; reports counts without reading content
    sniff.rs          file → best-guess format, with a confidence score
    formats/          generic parsers, reused by many sources
      vtt.rs  srt.rs  speaker_txt.rs  markdown.rs (frontmatter, wikilinks)  docx.rs  csv.rs
      enex.rs  textbundle.rs  notion_zip.rs  html_to_md.rs  prosemirror.rs
    sources/          one module per product: detection, input, mapping
      granola.rs  anarlog.rs  meetily.rs  macwhisper.rs  otter.rs  fireflies.rs  fathom.rs
      tldv.rs  circleback.rs  grain.rs  meetgeek.rs  fellow.rs  jamie.rs  readai.rs
      notion.rs  obsidian.rs  evernote.rs  bear.rs  apple_notes.rs  teams.rs  zoom.rs  meet.rs  …
    http.rs           reqwest client: per-source rate limiter, retry with backoff, cursor pagination
  fixtures/           a real (anonymised) sample export per format, used by golden tests
```

- Depends on `domain` and `store`, like `pipeline`. It does not depend on `tauri`. Progress goes out through `EventSink`.
- Audio transcoding is borrowed from `crates/audio` behind a small trait injected at setup, so `import` doesn't pull in capture code.
- No `cfg(target_os)` in this crate. Per-OS local paths come from a `KnownPaths` value built in `src-tauri` and passed in.

```rust
#[async_trait]
pub trait Source: Send + Sync {
    fn id(&self) -> ImportSourceId;
    /// Cheap: does this machine have the app, a CLI, a database? Never reads note content.
    async fn detect(&self, paths: &KnownPaths) -> Option<Detected>;
    /// Enumerate items and fidelity without writing anything. Streams progress for big inputs.
    async fn scan(&self, input: &ImportInput, cx: &ScanCx) -> Result<Scan, ImportError>;
    /// Produce notes one at a time so memory stays flat for a 5,000-note export.
    fn read<'a>(&'a self, scan: &'a Scan, cx: &'a ReadCx) -> BoxStream<'a, Result<ImportedNote, ItemError>>;
}
```

### Schema: `0004_imports.sql`

```sql
CREATE TABLE import_runs (
  id            TEXT PRIMARY KEY NOT NULL,
  source        TEXT NOT NULL,             -- 'granola', 'otter', 'files', …
  route         TEXT NOT NULL CHECK (route IN ('local','account','file','paste')),
  status        TEXT NOT NULL CHECK (status IN ('running','succeeded','partial','failed','cancelled','undone')),
  options_json  TEXT NOT NULL,
  imported      INTEGER NOT NULL DEFAULT 0,
  updated       INTEGER NOT NULL DEFAULT 0,
  skipped       INTEGER NOT NULL DEFAULT 0,
  failed        INTEGER NOT NULL DEFAULT 0,
  report_json   TEXT NOT NULL DEFAULT '[]', -- per-item skips and errors, written for people
  started_at    TEXT NOT NULL,
  finished_at   TEXT
);

CREATE TABLE import_links (
  source        TEXT NOT NULL,
  external_id   TEXT NOT NULL,
  note_id       TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  run_id        TEXT NOT NULL REFERENCES import_runs(id),
  content_hash  TEXT NOT NULL,             -- of the ImportedNote as read
  imported_at   TEXT NOT NULL,
  PRIMARY KEY (source, external_id)
);
CREATE INDEX import_links_note ON import_links(note_id);
CREATE INDEX import_links_run ON import_links(run_id);

ALTER TABLE notes ADD COLUMN transcript_timing TEXT NOT NULL DEFAULT 'exact'
  CHECK (transcript_timing IN ('exact','approximate','none'));
```

The note header reads "Imported from Granola · 4 Mar 2025" from `import_links`. There is no need for a column on `notes`.

### IPC

The commands live in `src-tauri/src/commands/import.rs`, which is a new area. All counts are `u32`, never `u64`.

| Command | Purpose |
| --- | --- |
| `import_sources() -> Vec<ImportSourceCard>` | The catalogue plus detection: id, name, group, route, input needed (`none \| api_key \| oauth \| file \| folder \| paste`), the fidelity that route gives, a help URL, and `detected: Option<{ label, count }>` |
| `import_save_key(source, key) -> ()` | Store the key in the keychain (`app.margin.desktop`, account `import.<source>`) and verify it with one cheap call. Like the provider keys, it never comes back over IPC |
| `import_connect(source) -> ()` | Run the OAuth flow (system browser, loopback, PKCE). Tokens go to the keychain |
| `import_scan(source, input: ImportInput) -> ImportPreview` | `ImportInput` is `Local`, `Account`, `Paths { paths }` or `Paste { text }`. Returns `scan_id`, total, date range, fidelity flags, folders, speaker names with counts, how many were already imported, likely duplicates, warnings and 5 sample notes |
| `import_start(scan_id, options: ImportOptions) -> ImportRun` | Options: `category_mode`, `self_names`, `link_people`, `save_frequent_speakers`, `include_audio`, `skip_external_ids` |
| `import_cancel(run_id)` / `import_undo(run_id)` | Undo goes through the destructive confirm card in the UI before it is called |
| `import_runs() -> Vec<ImportRun>` | History in Settings → Import |
| `import_forget_account(source)` | Delete the stored key or token |

There is one new event, `AppEvent::ImportProgress { run_id, phase: scanning|importing|finishing, done: u32, total: u32, current: Option<String> }`. The run also emits `data_changed { scopes: [Notes, Categories, Transcript, People] }`, at most once a second.

The UI side is `src/features/import/`, with `ImportView`, `SourceCard`, `ImportSheet`, `ImportPreview`, `useImport*` hooks and a drop-target hook mounted in the shell. It uses existing `ui/` components and adds no raw styling.

### Concurrency and limits

- **One import at a time.** Imports never run while recording: the scheduler's `is_recording()` check is reused, and an import in progress pauses when a recording starts.
- **Each source has a token bucket** set from its documented limits: Granola 5/s, burst 25; Fathom 30/min for transcript calls; Fireflies 30/min on Free; Read AI 100/min; Circleback 20/min on Free. A `429` with `Retry-After` is honoured. The UI shows an estimate when the wait is long.
- **Big files are streamed.** ENEX and Fireflies takeout ZIPs are never loaded into memory whole.
- **The writer commits one note per transaction**, so cancelling or crashing loses at most the note in flight. A resumed run skips anything already in `import_links`.

## Format parsers (shared)

These are worth building well because most sources reuse them.

| Parser | Recognised by | Notes |
| --- | --- | --- |
| **VTT** | `WEBVTT` header | `<v Name>` voice tags (Teams), `Name: text` cues (Zoom). Merge consecutive cues from the same speaker into one turn |
| **SRT** | Numbered blocks with `-->` | Speaker from a `Name:` prefix if present |
| **Speaker TXT** | Line patterns | Otter `Name  0:03` then text; `[00:01:02] Name: text`; `Name (00:01:02)`; plain `Name: text`. Pick the pattern that matches the most lines, and give a confidence score |
| **Markdown** | `.md` | YAML frontmatter (title, date, created, tags, attendees), `[[wikilinks]]` kept as text, embeds resolved. A `## Transcript` section in Margin's, Shadow's or Grain's layout becomes segments |
| **DOCX** | zip with `word/document.xml` | Convert to Markdown (headings, lists, bold), then try the speaker-TXT patterns on the transcript part |
| **CSV** | Header row | Granola CSV columns (**unverified: needs a sample**), Notion database CSVs |
| **ENEX** | `<en-export` | Streaming XML, ENML → Markdown, `<en-todo>` → `- [ ]`, `<en-crypt>` reported as skipped |
| **Notion zip** | 32-hex suffixed names | Strip IDs, rebuild hierarchy, rewrite links |
| **TextBundle** | `.textbundle` / `.bear2bk` | `text.md` + `assets/` + `info.json` dates |
| **ProseMirror JSON** | Anarlog `body_format = prosemirror_json` | Render to Markdown, the same direction as Margin's TipTap round-trip |

**How a dropped file is identified.** `sniff.rs` asks each source's matcher for a confidence score; matchers check things like ZIP entry names and CSV headers. The highest score wins, and the preview says what it decided ("Looks like an Otter export"). Below a threshold, the generic parser for that extension takes the file.

## Source adapters, phase 1 detail

### Anarlog (local, verified on this Mac)

- **Paths:** `~/Library/Application Support/anarlog/app.db`. Older installs used `~/Library/Application Support/hyprnote/` or `com.hyprnote.stable/`. Also check whether `anarlog` is on PATH.
- **Reading:** copy `app.db`, `-wal` and `-shm` to a temp dir and open the copy read-only, so the live app is never locked or disturbed.
- **Tables read:** `sessions` (id, title, started_at, ended_at, created_at, updated_at, folder_path, deleted_at), `session_documents` (session_id, kind, title, body_format = `prosemirror_json`, body, deleted_at), `transcripts` (session_id, started_at_ms, ended_at_ms, `words_json` = `[{id, text, start_ms, end_ms, channel, metadata}]`, `speaker_hints_json` = `[{id, word_id, type, value}]`), `session_participants` + `humans` (name, email). Skip `deleted_at IS NOT NULL` rows. Never read `e2ee_*`, `cloudsync_*` or `auth.json`.
- **Speakers:** apply the speaker hints to the words, then group the words into turns. The mic channel is self.
- **Audio:** `sessions/<id>/audio.mp3` when "Include audio" is on.
- **Schema drift:** prefer the CLI (`anarlog meetings export --format json`) when it is installed, and fall back to the database. Pin the tested schema by checking that the columns above exist, and fail with a clear message if they don't.

### Granola

- **API key** (Business plan and up): `GET /v1/notes?created_after=` with cursor pagination, then `GET /v1/notes/{id}?include=transcript`. Fields: `summary_markdown`, `private_notes_markdown`, `attendees`, `calendar_event`, `created_at`. Transcript: `speaker.source` (microphone|speaker) maps straight to mic|system, `speaker.name` gives the label, plus `start_time`/`end_time`.
- **CSV** (every plan): drop-in file. Map the title, summary and transcript columns. **Need a real CSV to confirm column names and transcript formatting before shipping.**
- **Detection:** `/Applications/Granola.app` exists. The card then says "Granola is on this Mac. Import with your API key, or export a CSV from Granola → Settings."

### Otter

- **Bulk ZIP** (Business plan) or individual TXT/DOCX/SRT files, all through the speaker-TXT, DOCX and SRT parsers. The Otter TXT pattern is `Speaker Name  m:ss` on its own line, followed by the text.
- **Summaries** are a separate export ("Export Summary"). When the user drops both files for the same conversation, pair them by title and date.
- **Enterprise API** (`api.otter.ai/v1/conversations`, 20 per page, `include=all`): phase 2, behind the same key-paste card.

### Fireflies

- **GraphQL with a personal key:** page `transcripts(limit: 50, skip, fromDate)`. Use `sentences` for turns (times in seconds → ms), `summary.overview` + `summary.action_items` for the body, `meeting_attendees` for attendees.
- **Pace for Free accounts:** 30 requests a minute and 500 a day. A 600-meeting account will not finish in one day, so the run pauses and resumes the next day on its own, and the progress pill says so.
- **Takeout ZIP:** a second adapter for Free users and for audio. Its layout is **unverified and needs a sample**.

### Fathom

- **REST** `X-Api-Key`, `GET /meetings?include_transcript=true&include_summary=true&include_action_items=true` with cursor pagination. Use `default_summary.markdown_formatted` as the body, `transcript[].speaker.display_name` and `timestamp` (HH:MM:SS, start only, so the end is the next turn's start), and `calendar_invitees`.
- Keep under 30 heavy calls a minute.

### File formats in phase 1

VTT (Teams, Zoom, Webex, Avoma), SRT, speaker TXT, Markdown folders (Obsidian, Shadow, Grain ZIP, Sembly, Google Docs exported as `.md`), DOCX, and the Notion zip.

## Privacy and security

- **Keys and tokens** live only in the OS keychain. When an import finishes, the card offers "Forget key"; the default is to forget unless "Keep importing new meetings" is on (phase 3). Keys never appear in logs, `options_json`, run reports or IPC responses.
- **Network** calls go only to the source's documented API host. The connect dialog names it ("Margin will connect to api.fireflies.ai"). File and local imports make no network calls at all.
- **Local reads** are read-only on copies. Each adapter names the exact tables or files it reads, and a test fails if an adapter opens anything else.
- **macOS permissions:** non-sandboxed apps' data under `~/Library/Application Support` is readable. App Store containers (such as MacWhisper's) trigger macOS's "access data from other apps" prompt, and Apple Notes' or Voice Memos' group containers need Full Disk Access. Those cards explain the prompt before triggering it and fall back to the export route if it is declined.
- **No LLM calls during an import.** After a large import, Margin may offer once: "Tidy titles and snippets for 214 imported notes with Margin?" That runs as an opt-in task on the user's configured provider, and its edits go through the normal tiers.
- **Imported text is untrusted.** Imported bodies and transcripts get the same quoted-data wrapping as live transcripts when the assistant reads them.
- **Consent:** importing someone else's meeting transcript is the user's own data, carried over. Margin adds no notice.

## Testing

- **Golden tests per format and source:** each `fixtures/<source>/` holds a real export, anonymised (names and content replaced, structure kept exactly), with an expected `ImportedNote` JSON snapshot.
- **Writer tests against in-memory SQLite:** check the field mapping, duplicate handling (unchanged, changed, edited in Margin), undo, resume after a crash mid-run, and untimed transcripts.
- **HTTP adapters against a mock server** (`wiremock`) replaying recorded responses: pagination, `429` with `Retry-After`, expired key, plan-gated `403`.
- **Local-database adapters** get a schema fixture built from that app's published migrations (Meetily, Anarlog) and a test that fails loudly when expected columns are missing.
- **UI:** vitest for the preview reducers and sniff-result copy. A mock-backend scenario in `dev:web` covers each route.

## Phasing

| Phase | Ships | Why |
| --- | --- | --- |
| **1** | Pipeline, schema, Import UI, drop-anywhere. Parsers: VTT, SRT, speaker TXT, Markdown/Obsidian folder, DOCX, Notion zip. Sources: **Granola** (API + CSV), **Anarlog** (local), **Otter** (files), **Fireflies** (API), **Fathom** (API), Teams/Zoom/Meet/Webex files | Covers the largest switching audiences and, through the generic parsers, most long-tail exports |
| **2** | Meetily, MacWhisper, Superwhisper, Shadow, Jamie, Limitless (time-boxed), tl;dv, Circleback, Fellow, Grain, MeetGeek, Read AI (OAuth), Notion API + meeting notes, Evernote, Bear, Apple Notes (JXA + Markdown), Fireflies takeout | Key-paste APIs are cheap once the HTTP layer exists |
| **3** | Generic MCP OAuth client (Krisp, Bluedot), Plaud CLI, Voice Memos, Quill, Keep, Logseq, OneNote, Slack canvases, Graph/Zoom/Google OAuth connectors, "Keep importing new meetings" (daily scheduled sync during a switch-over month) | Long tail and enterprise sign-ins with admin consent or verification hurdles |

Phase 1 fits after **Notes** in the build order. It needs the store and editor but nothing from recording or the assistant.

## Open questions

- **Fixtures to collect before writing parsers:** Granola CSV, Fireflies takeout ZIP, Otter bulk ZIP + summary export, Grain Markdown ZIP, Krisp bulk export, Shadow folder, Read AI TXT, Tactiq TXT.
- **Category depth:** should "Parent / Child" category names stay, or should Margin add nested categories? Flat is fine for v1.
- **Tags:** Margin has none. Writing a `Tags:` line keeps the data; a real tag model is a separate decision.
- **Continuous sync:** phase 3 proposes a time-limited sync to ease switching. Confirm we want that rather than one-shot imports only.
- **Terms:** reading another app's local database and using personal API keys is the user exercising access to their own data. Re-check each vendor's API terms before shipping its adapter, as the spec does for the Claude Code bridge.

## Sources

Researched 12 Nov 2026. Primary docs unless noted.

- **Granola:** [exporting notes](https://docs.granola.ai/help-center/sharing/exporting-notes), [API](https://docs.granola.ai/help-center/sharing/integrations/granola-api.md), [transcript](https://docs.granola.ai/api-reference/get-transcript.md), [MCP](https://docs.granola.ai/help-center/sharing/integrations/mcp.md), [local cache encryption](https://github.com/tomelliot/obsidian-granola-sync/blob/3a0612a/docs/CREDENTIALS.md) (third-party)
- **Anarlog:** [CLI](https://github.com/fastrepl/anarlog/blob/main/docs/reference/cli.mdx), [MCP](https://github.com/fastrepl/anarlog/blob/main/docs/reference/mcp.mdx), [its own importers](https://github.com/fastrepl/anarlog/blob/main/apps/desktop/src/imports/providers.ts). The local schema was checked on this machine.
- **Meetily:** [migrations](https://github.com/Zackriya-Solutions/meetily/tree/main/frontend/src-tauri/migrations), [db path](https://github.com/Zackriya-Solutions/meetily/blob/main/frontend/src-tauri/src/database/manager.rs)
- **Jamie:** [API](https://docs.meetjamie.ai/developers/api), [get meeting](https://docs.meetjamie.ai/developers/api/meetings/get-meeting.md)
- **MacWhisper:** [file locations](https://docs.macwhisper.com/article/54-file-locations)
- **Superwhisper:** [CLI](https://superwhisper.com/docs/get-started/cli.md)
- **Limitless:** [developers](https://www.limitless.ai/developers)
- **Plaud:** [CLI](https://docs.plaud.ai/plaud-mcp-cli/cli.md)
- **Krisp:** [MCP](https://help.krisp.ai/hc/en-us/articles/25396920405148-Krisp-MCP)
- **Bluedot:** [MCP](https://help.bluedothq.com/en/articles/14708332-bluedot-mcp-overview)
- **Notion:** [meeting notes query](https://developers.notion.com/reference/query-meeting-notes.md), [page markdown](https://developers.notion.com/reference/retrieve-page-markdown), [export](https://www.notion.com/help/export-your-content)
- **Otter:** [export](https://help.otter.ai/hc/en-us/articles/360047733634-Export-conversations), [bulk export](https://help.otter.ai/hc/en-us/articles/13829342669079-Bulk-export-conversations-audio-or-Takeaways), [API](https://help.otter.ai/hc/en-us/articles/36130822688279-Otter-ai-Public-API)
- **Fireflies:** [transcripts query](https://docs.fireflies.ai/graphql-api/query/transcripts.md), [limits](https://docs.fireflies.ai/fundamentals/limits), [account export](https://fireflies-knowledge-base.help.usepylon.com/articles/8915664468-how-to-export-your-fireflies-account-data)
- **Fathom:** [list meetings](https://developers.fathom.ai/api-reference/meetings/list-meetings), [limits](https://developers.fathom.ai/api-overview)
- **tl;dv:** [API](https://doc.tldv.io/)
- **Read AI:** [auth](https://support.read.ai/hc/en-us/articles/49380809380371-API-Keys-Authentication)
- **Circleback:** [API](https://circleback.ai/docs/api/llms-full.txt)
- **Fellow:** [recordings](https://developers.fellow.ai/reference/recordings)
- **Grain:** [bulk export](https://support.grain.com/en/articles/15297887-how-to-bulk-export-your-meetings-and-transcripts)
- **MeetGeek:** [transcript](https://docs.meetgeek.ai/api/api-reference/v1/transcript.md)
- **Avoma:** [download](https://help.avoma.com/downloading-a-meeting)
- **Sembly:** [notes export](https://help.sembly.ai/en/articles/81-sembly-meeting-notes)
- **Tactiq:** [download](https://help.tactiq.io/en/articles/9784666-how-to-download-your-tactiq-meeting-transcript)
- **Microsoft Teams:** [Graph transcripts](https://learn.microsoft.com/en-us/graph/api/calltranscript-get), [AI insights](https://learn.microsoft.com/en-us/microsoftteams/platform/graph-api/meeting-transcripts/meeting-insights)
- **Zoom:** [cloud recording](https://developers.zoom.us/docs/build/cloud-recording/), [AI Companion transcript](https://devforum.zoom.us/t/api-to-get-an-ai-companion-generated-transcript/142661) (forum)
- **Google:** [Meet artifacts](https://developers.google.com/workspace/meet/api/guides/artifacts), [Drive export formats](https://developers.google.com/workspace/drive/api/guides/ref-export-formats)
- **Webex:** [transcripts](https://developer.webex.com/docs/api/v1/meeting-transcripts)
- **Slack:** [canvases.getContent](https://docs.slack.dev/reference/methods/canvases.getContent)
- **Evernote:** [ENEX](https://evernote.com/blog/how-evernotes-xml-export-format-works)
- **Bear:** [backup](https://bear.app/faq/backup-restore/)
- **Obsidian:** [properties](https://obsidian.md/help/properties)
- **Apple Notes:** [NoteStore parser](https://github.com/threeplanetssoftware/apple_cloud_notes_parser)
- **Voice Memos:** [tsrp transcript](https://gist.github.com/Thomascountz/b84b68f0a7c6f2f851ebc5db152b676a) (third-party)
