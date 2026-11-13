# Cilica build spec

> v0.2, 11 Nov 2026. Source of truth for behaviour. The design system (docs/design-system/) is the source of truth for look and motion; where the two disagree on visuals (for example toast position), the design system wins.

A local-first desktop notes app with an AI assistant and a meeting summariser. The assistant can read and edit every note; the summariser records calls without a bot, transcribes them on-device with speaker labels and writes them up as notes. This page is the brief for agents designing and building it.

- Tauri 2 + Rust core
- macOS 14.4+ · Windows 10/11 · Linux (PipeWire)

- Local-first SQLite
- Records system + mic audio

## Decisions on the open questions

| Question | Decision |
| --- | --- |
| UI framework | **Tauri 2**: all logic in Rust crates, UI in React + TypeScript in the OS webview. Pure-Rust UIs (GPUI, Iced, Slint, Dioxus native) would mean building a rich Markdown editor, drag and drop, blur and spring animation from scratch. Tauri keeps the binary small (no bundled Chromium) and gives native window vibrancy through `window-vibrancy`. Anarlog uses the same split. |
| Meeting audio capture | Capture **two streams**, never integrate with Zoom: the **mic** (you) and **system output** (everyone else). This is app-agnostic, so Zoom, Meet, Teams, WhatsApp and Slack all work with no bot. Per-OS APIs are in Audio capture. |
| Claude subscription | Anarlog does not handle Claude tokens. It runs the user's installed `claude` CLI headless (`claude -p --output-format stream-json`), so Claude Code's own login is used. Copy that as an opt-in **Claude Code bridge**, but make API key the default. Anthropic's 2026 terms limit subscription use to Claude Code and claude.ai, so label the bridge experimental. See Providers. |
| AI guardrails | Adopt all six ideas, arranged as **permission tiers**: reads run freely, additive writes apply with undo, edits show a diff, destructive actions need confirmation. Everything is versioned, soft-deleted and logged. Add transcript prompt-injection isolation. See Guardrails. |
| Bottom sidebar | Listening toggle, Scheduled tasks, Saved people, AI activity, Trash, Settings. |
| Speech-to-text | **Local Whisper** (whisper.cpp) is the default engine, as in Anarlog's on-device mode. Audio never leaves the machine. Cloud services (Soniox, Deepgram, AssemblyAI, OpenAI, or any OpenAI-compatible endpoint) are an option with the person's own key, for computers that cannot keep up locally. |

## Stack

| Layer | Choice |
| --- | --- |
| Shell | Tauri 2: WKWebView (macOS), WebView2 (Windows), WebKitGTK (Linux). Plugins: global-shortcut, single-instance, updater, window-state, tray, autostart. |
| IPC | `tauri-specta` generates typed TS bindings for every command and event. No hand-written `invoke("…")` strings. |
| Frontend | React 19, TypeScript, Vite. **Motion** for all animation. Tailwind v4 over CSS-variable tokens. Radix primitives (unstyled) for accessible menus/dialogs. `dnd-kit` for drag into categories. `cmdk` under the command palette. Zustand for UI state, TanStack Query as the cache over IPC. |
| Editor | TipTap 3 (ProseMirror) with Markdown in/out. Markdown is the stored format. Anarlog also uses TipTap. |
| Storage | SQLite via `sqlx` with migrations; FTS5 for search. Secrets in the OS keychain via `keyring`. Meeting audio as Opus files with a retention setting (default 30 days). |
| Audio | `cpal` (mic), `cidre` (macOS Core Audio taps), `wasapi` (Windows loopback), `pipewire` (Linux). Resample to 16 kHz mono. Echo cancellation on the mic using system audio as the reference. |
| Speech-to-text | **Default: local Whisper** via `whisper-rs` (whisper.cpp; Metal / CUDA / Vulkan features) with Silero VAD chunking. The model downloads during first-run setup and keeps going in the background. Settings recommends a model for the computer (Turbo on Apple silicon and strong PCs, Small on typical laptops, Base plus a cloud suggestion on old machines); choosing one and clicking Download downloads it and switches to it. Optional cloud engines with the user's own key (keychain account `stt.<engine>`), behind one `Transcriber` trait: Soniox, Deepgram and AssemblyAI stream over WebSocket; OpenAI and OpenAI-compatible services take utterances cut at pauses. Choosing one first confirms that audio will go to that provider. Settings › Transcription has a Test that streams an 11 s public-domain speech clip (never the person's audio) to the service in real time and shows the words as they come back and how quickly the first ones arrived. **After the meeting** (on by default), the whole recording is transcribed again with Whisper Large v3 Turbo (beam search, full windows) and replaces the live transcript, unless that pass loses more than half the words. It runs only with local Whisper; a cloud service's transcript is kept, and the setting is hidden while one is chosen. **Speakers** are labelled live, on the call and on the microphone (Settings › Transcription › Label speakers): by **this computer** (default, with any engine), by **the service** (Soniox, Deepgram and AssemblyAI only; with another engine this falls back to this computer) or **off** (the mic is You and the call is Others). Each speaker becomes a "Person N" on the note, except the first voice on the microphone, which is You. Settings recommends this computer only where it keeps up comfortably (Apple silicon or 8+ cores, with 16 GB or more); elsewhere it says the service's labels are lighter. There is no second pass after the meeting. |
| Diarization | `ort` (ONNX Runtime), live: pyannote segmentation 3.0 every second over the last 10 s, a speaker-embedding model (CAM++ by default; Settings lists the voice models with one recommended for the computer) matched against the speakers heard so far, decided 3 s behind the audio. Lines Cilica is not sure of yet show "Pending". On three recorded four-person meetings it gave the right person for 94–99% of the speech it labelled, against 76–92% for Soniox's live labels. |
| LLM | `reqwest` + SSE streaming against the Anthropic Messages API with tool use. Defaults: `claude-sonnet-5-5` for assistant and summaries, `claude-haiku-5-5` for titles, snippets and prompt chips. |

### Performance budgets

- Installer under 20 MB, excluding downloadable speech models.
- Idle memory under 150 MB. Cold start to interactive under 1 s.

- Animations hold 60 fps (120 on ProMotion). Only `transform`/`opacity` animate; never width/height/top.
- Live transcript lags speech by under 3 s with the local engine.

## Installing on Windows

Windows downloads **Cilica Setup** (`Cilica_<version>_x64_Setup.exe`), a small frameless window over the same ridgeline as first-run setup, instead of a stock installer wizard. Three screens:

1. **Install Cilica**: one button. Options opens a panel with the folder (`%LOCALAPPDATA%\Cilica` by default), a desktop shortcut (off) and Open Cilica when you sign in (off). Cilica installs for the current user only, so Windows never asks for an administrator. When Cilica is already installed the screen becomes **Update** (or **Reinstall** for the same version) in its existing folder, keeping its shortcuts and login item.
2. **Installing Cilica**: a line for what is happening, the bar with its percentage, and a checked line per finished step. The taskbar button fills with the bar. The sun rises as it goes. Close waits until it is done. A failure says what went wrong, with Try again.
3. **Cilica is installed**: Open Cilica starts it on its own setup window; Close leaves it in the Start menu.

Updates Cilica finds itself install in the same window. On Windows the new version downloads in the background (or on Download in Settings › General) and waits; the toast and Settings offer Restart, which closes Cilica and opens Setup straight on **Updating Cilica** ("From 0.5.1 to 0.5.2. Your notes and settings stay as they are."), with a line for waiting for Cilica to close. When it is done it shows **Cilica is updated** for a moment and opens Cilica again. A failure stays on screen with Try again. The update Cilica downloads is Cilica Setup itself, so copies from before Setup existed update through the same window too.

Removing Cilica from Settings › Apps opens the same window: **Remove Cilica?** with one switch, off by default, to also delete notes, recordings, settings and saved API keys (it names the folder they are kept in), then progress as the sun sets, then **Cilica is removed**.

Under the window, Tauri's own NSIS installer and uninstaller do the work, silently, so the Installed apps entry, Start menu shortcut and updater behave exactly as before. Without WebView2 (rare on Windows 10), Setup opens Tauri's standard installer instead, which installs WebView2 itself. Cilica Setup is the only installer published for Windows; there is no separate NSIS or MSI download. The updater still finds, downloads and verifies updates; Setup is the package it downloads.

## First launch

Setup is its own small window (560 × 700, fixed size), not a dialog in the app. The main window stays hidden until setup finishes or is closed. Over background art (a pixel ridgeline drawn in code, dawn in light and night in dark), one column:

1. **Welcome**: one line on what Cilica is (a notes app with an AI assistant and a meeting summariser) and four features from the README in that order (notes, the assistant, meeting summaries, import), then Set up Cilica. Once you start, the welcome steps aside so each step title is the only heading.
2. **Let Cilica hear your meetings**: microphone, system audio and Accessibility (optional) as buttons that turn into confirmations, plus Record when a meeting starts (Ask me first or Start on its own). Only the microphone is needed to go on.
3. **Transcribe on this Mac**: Mostly English or Several languages picks the recommended Whisper model; the download continues in a pill at the bottom of the window and after setup.
4. **Connect Cilica to a model**: Anthropic key, Ollama, an OpenAI-compatible server (address, model, key) or Claude Code (with the terms warning). Keys are checked before they are kept.
5. **Bring your notes**: apps found on this computer with their counts; picked ones import in the background.
6. **Make it yours**: theme and note typeface. Its button, Open Cilica, finishes setup. There is no ending page.

Finished steps collapse to one line with a check (grey when skipped) and reopen when clicked. Every step can be skipped. Finishing or closing the window marks setup done, writes a welcome note with the three main shortcuts (first run only) and opens the main window on it. A quiet ambient pad is available from the speaker button and starts muted. Settings › General › Run setup again opens the window over Cilica.

## Layout

### Left sidebar

- **Top row**: New note, New category (inline-editable name), Record. Below it, a search field that opens the command palette (`⌘K`), which searches notes, transcripts, conversations with Cilica and tasks (see [Search](#search)).
- **Note rows**: title, relative date, one-line AI snippet, a mic glyph for meeting notes, a pulsing red dot while recording.

- **Categories**: drag notes in or out (dnd-kit, animated drop), drag to reorder categories, click the header to collapse. Uncategorised notes sit at the top under “Notes”.
- **Bottom**: Graph (`⌘G`), Scheduled tasks (name, next run, status dot), Saved people, AI activity, Trash, Settings, and a Listening row that toggles auto-detect and shows a live mic level during recording.

- **Selecting notes**: `⌘`-click adds or removes a note (one is enough to highlight it), Shift-click selects everything between (across categories, skipping folded ones), `⌘A` selects every note shown, Shift+↑/↓ grows the selection. A plain click opens a note and clears the selection, as does Esc or clicking anywhere outside the note rows; the open note is never selected by itself. `⌘⌫` trashes the selection instead of the open note. Right-clicking one of several selected notes opens a menu for all of them: move to a category (as one block, order kept), ask Cilica about them (a draft linking each note), or move to Trash (one Undo brings them all back). Dragging a selected note carries the whole selection. Trash selects the same way, to restore or delete several at once (delete asks first).
- Collapses fully with `⌘\`.

### Note view

- Header: editable title, a **SegmentedSlider** (Notes ↔ Transcript, animated thumb, `⌘E`), recording timer and Stop while live, overflow menu (export Markdown, copy, history, move to trash).
- **Notes panel**: TipTap Markdown editor. The user can type during a meeting; those jottings are kept and woven into the AI summary, as Granola does.
- **Spelling**: the webview's own checker underlines misspelt words as you type in the body, the title and the assistant box; right-click a word for suggestions. Code, inline code and tags are never checked. WebKit leaves the checker off for embedded webviews, so the shell turns it on (macOS: the `WebContinuousSpellCheckingEnabled` default before the first webview; Linux: the WebKitGTK context, in the `LANG` language). Settings › General › Writing › Check spelling (on by default) sets `spellcheck` on `<html>`, which every text box inherits. Words already in a note are only checked once you edit near them.

- **Transcript panel**: segments grouped by speaker with timestamps, auto-scrolling while live (pauses when the user scrolls up). Click a speaker chip to rename or link a Saved person.
- **History**: version list with author (you or AI) and a diff against current; one-click restore.
- **Connections** (under the body): the note's local graph (1 or 2 steps out), notes that link here with the line they link from, and unlinked mentions of the title with a Link button. See Links and the graph.

### Right sidebar: assistant

- Toggled with `⌘J`; the panel animates in and pushes the editor (it does not overlay it).
- A context line at the top shows what the assistant can see (“Viewing: Acme kickoff · Transcript”).

- Empty state: 3–4 prompt chips generated from context. Meeting note: action items, follow-up email, per-person commitments. Sidebar or category selected: weekly digest, find duplicates. Chips are cached per note and refreshed after edits.
- Tool calls appear inline as compact rows (“Searched 14 notes”). Proposed edits render as diff cards with Apply / Reject.

## Search

The command palette reads what is typed as words plus filters, and shows each filter it understood as a chip under the input. Clicking a chip removes it and the words it came from; "Search the words instead" turns the reading off for that query.

- **Dates** (local time): today, yesterday, this/last week, month or year, last 7 days, 3 days ago, weekdays (the most recent one), 14 Nov, Nov 14th 2025, 2026-11-14, 14/11 (day or month first by the computer's locale), November, in 2025, since/before/until a date, between two dates, from one date to another, recently (two weeks). "may" and a bare year are only dates after a preposition.
- **Times**: 3pm, 15:30, at 3, around 3pm (15 minutes before to an hour after), 2-4pm, between 2 and 4, after 6pm, this morning, yesterday afternoon, last night, tonight. With one day they narrow it; without, they apply to every day.
- **Which date**: by default a note matches on when its meeting started (or it was made) or when it was last edited. "created", "edited" (including saved versions), "met" or "recorded", and "deleted" before a date pick one. A meeting search with a date uses when the meeting started.
- **Meetings**: with Sam (Saved people, first name when unique, "me"), on Zoom or "zoom calls" (meeting apps), longer than 30 min, under 10m, long or quick calls (45 and 15 minutes), with or without a recording, with quotes. "meetings" and "calls" only filter alongside another filter; alone they are words.
- **Structure**: in Work (category), `#tag`, links to a note title or `[[Title]]`, imported from Notion, in Trash or deleted.
- **Syntax**: `in:`, `with:`, `tag:`, `app:`, `from:`, `is:meeting|note|deleted`, `has:` and `no:recording|quotes`, `before:`, `after:`, `since:`, `until:`, `on:`, `created:`, `edited:`, `met:`, `deleted:`, `links:`, `longer:` and `shorter:`, `"exact phrase"`, `-word`. Values with spaces go in quotes.
- **Suggestions**: while a word is typed the palette offers matching people, categories, tags, apps and import sources as filters.
- **Results**: with filters, matching notes newest first by the date asked about, with that date shown; Trash matches in their own group (they open Trash). Without filters, as before plus conversations with Cilica (by title and message text) and tasks (by name and prompt).
- **Backend**: `find_notes(NoteQuery)` applies everything in SQL on top of FTS5; `search_facets` lists tags, meeting apps and import sources; `search_threads` searches conversations. The assistant's `search_notes` tool takes the same filters (after, before, date_field, kind, person, tag, meeting_app, in_trash).

## Links and the graph

Notes link with Obsidian's Markdown, so an imported vault keeps its links and Cilica's notes open cleanly in Obsidian.

- **Syntax**: `[[Title]]`, `[[Title|shown text]]`, `[[Title#Heading]]`, `[[Title#^block]]`, `[[folder/Title]]`, `![[Title]]` (embed, shown as a card with the note's first line), `[text](Title.md)` and `#tags` (nested `#area/topic`; not all digits; after a space or at line start). Links in code, `%%comments%%` and links to attachments are ignored.
- **Resolving**: the newest note with that title (case-insensitive), else a Saved person with that name, else a note not created yet (dashed). Clicking one opens it, opens Saved people, or creates the note in the current note's category.
- **Writing links**: typing `[[` suggests notes and people; typing `]]` or pasting turns text into a link; Backspace right after a link turns it back into text. Renaming a note rewrites `[[Old title]]` in other notes, unless another note still has the old title.
- **Graph** (`⌘G`, sidebar): every note, plus people, tags and notes not created yet. Dots are sized by links; people heard in a meeting join it with a dotted line (read from the transcript, never written into the note). Hover lights a node's neighbours. Find sits in the band; the options (show people, tags, notes not created yet; colour by type or category) start hidden behind one button. Forces and sizes are tuned, not exposed.
- **Cost**: saving a note does no link work; a trigger marks it stale and the next graph or backlinks read re-reads only stale notes (≈0.1 ms for one edit, ≈26 ms to build the graph for 5,000 notes). The canvas layout runs only while it settles or while you drag, and draws nothing when idle.
- **Known limit**: bold or italic wrapped around a link (`**[[Title]]**`) keeps the link but drops the emphasis on save; the editor's Markdown serializer cannot wrap an inline link node in a mark.

## Recording flow

- **Trigger.** Manual: Record button, `⌘⇧R` (global hotkey) or the tray menu. Automatic: the detector reports an allow-listed meeting app holding the mic for 5 s. The setting chooses between *auto-start* and *ask first* (an action toast), default ask first.
- **New note.** Titled “{App} meeting · {time}” and opened on the Transcript panel. A toast confirms with a Stop action.

- **Capture.** Mic and system streams start in parallel, are echo-cancelled and resampled, and are written to disk as they arrive, so a crash loses nothing.
- **Live transcript.** VAD splits speech into utterances (a 700 ms pause ends one; past 6 s a 250 ms breath does; 12 s at most); each decode is primed with the stream's recent text; mic and call audio each have their own decoder state. The engine streams partial then final segments to the UI as events. With speakers labelled on this computer, a segment is stored without a speaker and shows **Pending** until speaker detection has heard it (a few seconds), then gets You or Person 1, 2…; when two speakers turn out to be one person their lines are joined. With labels off, mic segments are **You** and system segments **Others**.

- **End.** Manual Stop, or the meeting app releases the mic for 15 s. A toast offers “Keep recording” for 10 s.
- **Finalize.** Lines still pending go to the nearest speaker; speakers nobody spoke as are dropped; voices are matched against Saved people (named when Cilica is sure, suggested when it is less sure; it learns only from confirmations). The careful Whisper pass keeps who said what from the live transcript. Then the LLM writes the summary into the Notes panel, above any user jottings: Summary, Decisions, Action items (owner + due), Open questions. The view slides to Notes.

### Speakers and Saved people

- Each Person 1…n is a cluster of speaker embeddings for that meeting. Naming one offers “Save as person”, which stores the name and the speaker's voice as a first sample.
- **You** is a Saved person that is always there (it cannot be deleted or merged away). Your mic speaker is linked to it, and your voice is learnt the same way as anyone's.
- **Learning.** Picking a Saved person for a speaker (or confirming Cilica's guess, or answering Yes to a suggestion) adds that speaker's voice as one of the person's samples. Picking someone else, or unlinking, takes that sample back out, so a correction untrains the wrong person. Cilica's own guesses teach nothing.
- **Recognising** (Settings › Transcription › Recognise saved voices, on by default). Live, once a labelled speaker has said about 3 s (then 8 s and 15 s), Cilica makes a voiceprint on this computer (always CAM++, whichever voice model tells speakers apart) and compares it with each Saved person's samples; at or above 0.72 cosine similarity it names them (one speaker per person; You only on the mic). Between 0.6 and 0.72 it only asks after the meeting: “Is Person 2 Sam?”.
- Saved people page: name, voice sample count, meetings they appeared in, rename/recolour/merge/forget voice/delete. Forget voice drops a person's samples and keeps them and their meetings.
- In-person meetings (one mic, many speakers): the mic channel is told apart live as well.

## Audio capture and meeting detection

Based on Anarlog's `audio-actual` and `detect` crates. Every platform branch lives in the `audio` and `detect` crates behind one trait; no `cfg(target_os)` anywhere else.

| OS | System audio | Meeting detection | Permissions |
| --- | --- | --- | --- |
| macOS | Core Audio **process tap** (macOS 14.4+): a global mono tap that excludes our own process, wrapped in a private aggregate device (via `cidre`). A tap can also target specific PIDs to capture only the meeting app. | Property listener on the input device's “running somewhere” state, plus the per-process input list to see *which* app holds the mic. Accessibility API reads window titles to spot Meet/Teams in browsers. | Microphone (`NSMicrophoneUsageDescription`). System Audio Recording (`NSAudioCaptureUsageDescription`). Accessibility (optional, browser detection only). Guided onboarding checks each one. |
| Windows | **WASAPI loopback** on the default render device. Per-app capture via process loopback on Windows 11. | Enumerate capture audio sessions on the default mic (`IAudioSessionManager2`) and map them to process names. | Microphone privacy setting only. Loopback needs no prompt. |
| Linux | **PipeWire** monitor of the default sink (PulseAudio monitor source as fallback). | PulseAudio/PipeWire source-outputs with `application.name`. | None natively. Flatpak needs the PipeWire portal. |

**Default meeting-app allow-list** (editable in Settings): Zoom, Microsoft Teams, Google Meet (browser), Slack huddles, WhatsApp, FaceTime, Webex, Discord, Around. Zoom gets no special handling: it is just audio out of the speakers and audio into the mic.

> **Consent:** Recording others may require their consent depending on jurisdiction. The first recording shows a one-time notice, and Settings has an optional “post a recording notice to chat” reminder.

## AI assistant

### Context, without being told

Each turn starts with a compact `` block built by the frontend: open note id and title, active panel, selected text, selected category, search query, recording state. Larger content (note bodies, transcripts) is fetched by the model through tools, so turns stay cheap.

### Tools

| Tier | Tools |
| --- | --- |
| Read | `search_notes`, `get_note`, `get_transcript`, `list_categories`, `list_people`, `list_tasks`, `get_note_history` |
| Additive | `create_note`, `create_category`, `move_note`, `rename_category`, `label_speaker`, `create_task` |
| Edit | `edit_note` (takes a patch, never a full rewrite), `rename_note` |
| Destructive | `trash_note`, `merge_notes`, `delete_category`, `delete_person`, any write that takes a turn past 5 notes |

The tool registry lives in the `agent` crate and is the only write path for AI. The same registry is exposed over MCP for the Claude Code bridge.

## Guardrails

### Applied per tier

- **Read**: runs freely.
- **Additive**: applies at once, with an Undo toast.

- **Edit**: a diff card; accept the whole change or per hunk.
- **Destructive**: a confirm card naming every affected note. This one cannot be switched off.
- **Bulk moves**: in a chat turn, once moves pass 5 notes, every further move joins one grouped card (notes listed under each destination category). The card grows as Cilica works and is confirmed once; applying it is one step with one Undo.

### Always on

- **Version history**: snapshot before every AI write.
- **Soft delete**: Trash keeps items 30 days.

- **Audit log**: AI activity lists every tool call with arguments, result and Undo.
- **Turn undo**: `⌘Z` reverts a whole AI turn as one step.

- **Prompt-injection isolation**: transcripts and note bodies reach the model wrapped as quoted data. A participant saying “delete all my notes” must never get past the destructive-tier confirmation, whatever the model decides.
- **Scheduled tasks only propose**: they run without the user present, so every change they suggest waits as a proposal.

- **Limits**: guards against a runaway turn, not limits real work should reach: 40 model steps, 200 write calls and 1,000 distinct notes per turn. No network or shell tools.
- Settings can promote Edit to auto-apply (still versioned). Destructive cannot be promoted.

## AI providers and auth

| Provider | How it works |
| --- | --- |
| Anthropic API key | **Default.** Key in the OS keychain. Direct Messages API with streaming and tool use. |
| OpenAI-compatible | Base URL + key: OpenAI, OpenRouter, Groq, LM Studio. |
| Ollama | Local models, no key. Assistant quality varies with model tool-use support. |
| Claude Code bridge | **Experimental, opt-in.** Detect `claude` on PATH and check its auth status. Spawn `claude -p --output-format stream-json` with `--resume` for threads and `--mcp-config` pointing at our local MCP server, with allowed tools limited to ours (no Bash, no file edits). The app never reads or stores Claude credentials. |

> **Policy risk:** Since February 2026, Anthropic's consumer terms say Free/Pro/Max OAuth tokens may only be used in Claude Code and claude.ai, and in April 2026 subscription access was cut off for third-party harnesses. Running the user's own CLI is how Anarlog does it, but it may still breach those terms and put the user's account at risk. Re-check Anthropic's current terms before shipping this path, and show the warning in the opt-in dialog.

## Scheduled tasks

- A task has a name, prompt, scope (all notes, a category, or the last N days), schedule (every N days, weekly on a day, or daily at a time), and an on/off switch.
- The runner lives in Rust: it stores `next_run_at` and runs missed tasks at the next launch. It never runs while recording.

- A finished run raises a toast and puts a badge on the task in the sidebar. Opening it shows the **report view**.

### Default: Notes review (every 3 days, enabled)

- **Merge**: groups of notes that cover the same topic, with a preview of the merged note.
- **Trim**: content that could be dropped, shown in the DiffView component (git-style red/green, line numbers in mono).

- **Delete**: notes that look stale or empty, with the reason.
- Each suggestion has Apply / Dismiss; “Apply all” still goes through the destructive confirm card. Dismissed items are remembered so they are not suggested again.

## Keybinds

`⌘` means Ctrl on Windows and Linux. A single registry drives the handlers, the command palette labels and the `⌘/` cheat sheet. All keybinds are rebindable in Settings.

| Keys | Action |
| --- | --- |
| `⌘K` | Command palette: notes by words, dates, people and more; conversations, tasks, actions, settings |
| `⌘F` | Find in current note |
| `⌘N` / `⌘⇧N` | New note / new category |
| `⌘⇧R` | Start or stop recording (global, works when the app is in the background) |
| `⌘E` | Swap Notes ↔ Transcript |
| `⌘G` | Open the graph |
| `⌘J` / `⌘\` | Toggle assistant / toggle left sidebar |
| `⌘Z` / `⌘⇧Z` | Undo / redo, including whole AI turns |
| `⌘⌫` | Move note to Trash |
| `⌘,` / `⌘/` | Settings / keybind cheat sheet |
| `Esc` | Close the topmost layer |

## Toasts

An in-house module at `src/lib/toast/`, modelled on the behaviour of [Sileo](https://sileo.aaryan.design/docs) (MIT): spring physics and a gooey morph from a compact pill into an expanded card. Depends only on React and Motion. Re-implement it; do not vendor Sileo.

```
toast("Saved")
toast.success("Summary ready", { description: "Acme kickoff" })
toast.error("Mic unavailable", { description: "Choose another input in Settings." })
toast.action("Zoom meeting detected", { action: { label: "Record", onClick } })
toast.undo("Moved 3 notes to Client work", { onUndo })
toast.promise(summarise(), { loading: "Summarising…", success: "Summary ready", error: "Summary failed" })
```

- Rust can raise toasts through a typed `toast` event, so backend jobs (scheduler, detector, STT errors) use the same component.
- Top-center. At most 3 visible, the rest queue. New toasts spring in and the stack reflows.

- Timing: 4 s default, 8 s for errors; toasts with an action stay until 6 s after the pointer leaves. Timers pause on hover and on window blur. Swipe or `Esc` dismisses.
- Reduced motion uses a plain fade. Colours, surface and icon styling come from the design system.

## Data model

```
notes            id, title, body_md, snippet, category_id?, kind(manual|meeting),
                 meeting_app?, started_at?, ended_at?, created_at, updated_at, deleted_at?
note_versions    id, note_id, body_md, author(user|ai), ai_action_id?, created_at
categories       id, name, position, collapsed
segments         id, note_id, speaker_id, channel(mic|system), start_ms, end_ms, text, is_final
speakers         id, note_id, label("Person 2"), person_id?, embedding
people           id, name, voiceprint (mean of samples), sample_count, created_at, is_self
voice_samples    id, person_id, speaker_id?, embedding, created_at
tasks            id, name, prompt, scope_json, schedule_json, next_run_at, enabled, is_default
task_runs        id, task_id, started_at, finished_at, status, report_json
proposals        id, source(chat|task), kind, payload_json, status(pending|applied|dismissed)
ai_actions       id, thread_id, turn_id, tool, args_json, tier, status, undo_json, created_at
threads/messages chat history per thread
notes_fts        FTS5 over title, body_md, transcript text
note_links       note_id, kind(link|embed|tag), target_key, target   -- read from body_md, rebuilt lazily
settings         key, value_json        -- secrets live in the OS keychain, never here
```

## Codebase layout

```
apps/desktop/
  src-tauri/          # thin shell: commands, events, windows (main, setup, island), tray
apps/setup/           # Cilica Setup: the Windows install and uninstall window around Tauri's NSIS installer
  src/
    app/              # root layout, providers, keybind registry, motion presets
    features/         # sidebar, note, transcript, assistant, people, tasks, activity, trash, settings
    ui/               # design-system components (the only place raw styling lives)
    lib/toast/        # in-house toast library
    ipc/              # generated by tauri-specta, never edited by hand
crates/
  domain/             # shared types: Note, Category, Segment, Person, Task, AiAction
  store/              # SQLite, migrations, FTS, versions, trash
  audio/              # mic + system capture, AEC, resampling (macos.rs / windows.rs / linux.rs)
  detect/             # meeting detection per OS
  stt/                # Transcriber trait: whisper_local, cloud adapters
  diarize/            # live speaker detection (segmentation, embeddings), voiceprints
  llm/                # Provider trait: anthropic, openai_compat, ollama, claude_code
  agent/              # tools, context builder, tiers, proposals, audit, undo
  scheduler/          # task runner, default Notes review
  mcp/                # exposes agent tools over MCP for the Claude Code bridge
```

- Each crate has a short README: purpose, public API, what it must not depend on. `domain` depends on nothing app-specific; `src-tauri` only wires crates together.
- A feature folder owns its components, hooks and state. UI components never call IPC directly; feature hooks do.

- Every crate is testable without a window: audio and STT run against recorded fixture WAVs, the agent against a mock provider.

## Build order

- **Foundation.** Tauri shell, vibrancy window, tokens, motion presets, `ui/` components, toast library, keybind registry, command palette.
- **Notes.** Store, sidebar with categories, drag and collapse, FTS search, TipTap editor, version history, Trash.

- **Manual recording on macOS.** Mic + process tap, AEC, local Whisper, live transcript, end-of-meeting summary.
- **Speakers.** Diarization, speaker chips, Saved people, voiceprint suggestions.

- **Assistant.** API-key providers, view context, tool registry, tiers, diff cards, AI activity, turn undo.
- **Auto-detect and other platforms.** Meeting detection on all three OSs, Windows and Linux capture, permissions onboarding.

- **Scheduled tasks.** Runner, task editor, Notes review report with DiffView.
- **Ship.** Claude Code bridge (experimental), updater, code signing and notarisation, performance pass against the budgets.

## Still open

- **Design system**: palette, glass treatment, motion presets, type and components. Coming next as its own document.
- **Out of v1 unless you say otherwise**: calendar integration, cloud sync, sharing, mobile, custom summary templates.

Sources: [fastrepl/anarlog](https://github.com/fastrepl/anarlog) (MIT; crates `audio-actual`, `detect`, `claude`, `agent-access`, `pyannote-local`) · [Sileo docs](https://sileo.aaryan.design/docs) · [WinBuzzer on the Feb 2026 terms](https://winbuzzer.com/2026/02/19/anthropic-bans-claude-subscription-oauth-in-third-party-apps-xcxwbn/) · [Hongkiat on the April 2026 enforcement](https://www.hongkiat.com/blog/anthropic-closed-openclaw-loophole/)
