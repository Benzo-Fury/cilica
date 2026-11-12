# Architecture

```
apps/desktop/
  src-tauri/src/
    lib.rs            builder, plugin list, the command list (specta), setup order
    core.rs           Core { db, events, data_dir }: managed state every command can reach
    events.rs         BackendEvent(AppEvent): the single typed backend → UI event
    commands/
      notes.rs        notes, categories, versions, trash, search
      settings.rs     settings, app info, window
      recording.rs    recording, permissions, speech models, transcripts, speakers, people
      assistant.rs    threads, turns, proposals, audit log, providers
      tasks.rs        scheduled tasks and reports
      import.rs       Settings › Import: detection, previews, imports, keys (keychain `import.<provider>`)
  src/
    app/              root layout, providers, keybind registry, motion presets
    features/         sidebar, note, transcript, recorder, assistant, people, tasks, activity, trash, settings, onboarding, palette, import
    ui/               design-system components (the only place raw styling lives)
    lib/toast/        in-house toast library
    ipc/              bindings.ts (generated) + client.ts (real or mock backend) + event bus
    styles/           tokens.css (generated), theme.css (Tailwind ↔ tokens), components.css (design-system classes)
crates/
  domain/             shared types, AppError, AppEvent + EventSink, MeetingSummarizer trait
  store/              SQLite (sqlx), migrations, FTS5. One module per schema area, each `impl Db`
  audio/              mic + system capture, echo cancellation, resampling, Opus/WAV writing
  detect/             meeting detection per OS
  stt/                Transcriber trait: whisper_local (+ model catalogue, download, hardware recommendation), cloud adapters (Soniox, Deepgram, AssemblyAI over WebSocket; OpenAI-compatible over HTTP)
  diarize/            VAD, segmentation, embeddings, clustering, voiceprint matching
  pipeline/           one recording session end to end: capture → VAD → STT → store → events → diarize → summarise
  llm/                Provider trait: anthropic, openai_compat, ollama, claude_code
  agent/              tool registry, view context, tiers, proposals, audit log, undo, summariser, prompt chips
  scheduler/          task runner, the default Notes review
  mcp/                the agent's tools over MCP (stdio) for the Claude Code bridge
  import/             notes from other apps: file formats, local databases, API-key accounts, preview, re-import, undo (docs/import.md)
```

## The contract

Three things are shared by everyone and change only with the orchestrator's agreement:

1. **`crates/domain`**: every type that crosses a crate boundary or IPC.
2. **`crates/store/migrations/0001_init.sql`**: the schema. Additions go in a new `0002_*.sql`.
3. **The command list** in `src-tauri/src/lib.rs` and the signatures in `commands/*.rs`, which generate `src/ipc/bindings.ts`.

If you need a change to any of them, make the smallest additive change in your own area (a new field with a serde default, a new command in your own module, a new migration file), regenerate bindings, and say so in your final report. Never rename or remove something another area uses.

## Backend → UI events

Crates receive an `Events` handle (`Arc<dyn EventSink>`) and call `events.emit(AppEvent::…)`. The shell forwards each as `BackendEvent`. The UI listens once (`events.backendEvent.listen`) and fans out by `payload.type`:

- `data_changed { scopes, note_id }`: invalidate TanStack Query keys. Emit this after any write the UI did not make itself (AI, scheduler, pipeline).
- `recording_state_changed`, `audio_levels`, `transcript_segment`, `note_body_stream`, `meeting_detected`, `meeting_ended`, `model_download`, `assistant`, `task_run_finished`, `settings_changed`, `toast`, `import_progress`, `open_note` (the main window opens that note; sent when setup finishes).

## Startup

`lib.rs` opens the database (`Core::init`), then calls each `commands::<area>::setup(app, &core)` in order: settings, notes, recording, assistant, tasks. Each area builds its own service there and registers it with `app.manage(...)`; commands take it as `State<'_, YourService>` alongside `State<'_, Core>`. The main window starts hidden; the UI calls `showMainWindow` after first paint.

Every window loads the same bundle; `main.tsx` picks the root from `?window=`: the main app, `island` (the Dynamic Island) or `onboarding` (first-run setup). On a first launch (`onboarding_done` false) `shell::setup` also builds the setup window (`shell/onboarding.rs`), and `showMainWindow` leaves the main window hidden while it is open. `finish_onboarding` (Open Margin, or closing the window) marks setup done, writes the welcome note (`Db::create_welcome_note`), emits `open_note`, closes setup and shows the main window. `open_onboarding` reopens setup from Settings.

## Cross-area seams

- **Recording → summary.** `pipeline` does not depend on `agent`. It takes an `Arc<dyn MeetingSummarizer>` (trait in `domain`, implemented in `agent`), injected in `commands::recording::setup` from a service the assistant area manages.
- **Scheduler ↔ recording.** Scheduled tasks never run while recording: the scheduler checks a `fn is_recording() -> bool` closure supplied at setup.
- **AI writes** go only through the `agent` tool registry, which snapshots a version, writes the audit row and emits `data_changed`.

## Ownership (who edits what)

| Area | Files |
| --- | --- |
| UI | `apps/desktop/src/**` (except `ipc/bindings.ts`, generated), `apps/desktop/index.html`, frontend config |
| Core | `crates/store/src/{lib,notes,settings}.rs`, `src-tauri/src/{lib,core,events}.rs`, `src-tauri/src/commands/{notes,settings}.rs`, `src-tauri/{tauri.conf.json,capabilities,Info.plist,Entitlements.plist}`, tray, windows, vibrancy, global shortcuts |
| Audio | `crates/{audio,detect,stt,diarize,pipeline}`, `crates/store/src/transcripts.rs`, `src-tauri/src/commands/recording.rs` |
| AI | `crates/{llm,agent,scheduler,mcp}`, `crates/store/src/{assistant,tasks}.rs`, `src-tauri/src/commands/{assistant,tasks}.rs` |
| Import | `crates/import`, `crates/store/src/imports.rs`, `crates/store/migrations/0004_imports.sql`, `src-tauri/src/commands/import.rs`, `src/features/import`, `src/ui/ImportParts.tsx` |

Shared files (`Cargo.toml` manifests, `src-tauri/src/lib.rs` command list, `crates/domain`): make small additive edits only, and re-read the file right before editing because another agent may have just changed it.

## Seams already stubbed in code

| Seam | Provided by | Used by |
| --- | --- | --- |
| `Db` methods in `crates/store/src/notes.rs` (signatures fixed, bodies `todo!()`) | Core | AI (tools, undo), Audio (pipeline) |
| `Db` methods in `crates/store/src/transcripts.rs` (signatures fixed) | Audio | AI (tools, summariser) |
| `commands::recording::toggle_recording(app, trigger)` | Audio | Core (global hotkey, tray) |
| `commands::recording::is_recording(app)` | Audio | AI (scheduler) |
| `commands::assistant::summarizer(app)` | AI | Audio (pipeline, at end of meeting) |

Until the owner lands the real body, code that calls a seam compiles but the call fails or panics at run time (`todo!()`), so test your own area against in-memory fakes or your own fixtures, not against another area's unfinished code.
