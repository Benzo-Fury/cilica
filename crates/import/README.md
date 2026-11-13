# cilica-import

Bring notes in from other apps (see `docs/import.md`): about 45 apps across notes apps, meeting note takers and meeting platforms, plus any Markdown, Word, text, transcript, CSV, Evernote or zip export.

- `Importer::{detect, detect_all}`: what's on this computer without reading anything heavy. Obsidian vaults come from Obsidian's own `obsidian.json`; Anarlog's and Meetily's SQLite databases are counted on a private read-only copy; for other apps only the installed `.app` is checked.
- `Importer::scan` reads a source into `ImportedNote`s and returns an `ImportPreview`. Nothing is written yet. Inputs are `Local` (a detected place), `Paths` (files, folders, zips), `Account` (the API key saved in the keychain, passed in through `Secrets`) and `Paste`.
- `Importer::start` imports in the background, oldest first, writing one note per store transaction. Progress arrives as `AppEvent::ImportProgress`. `cancel` stops after the note in flight. `undo` moves the notes a run created to Trash.
- Importing again is safe. Every note has a link holding `(provider, external_id)`, a hash of the source and a hash of what Cilica wrote.
  - Unchanged items are skipped.
  - Changed items are updated, with the old body kept as a version.
  - Items the person edited in Cilica are left alone.
  - Notes in Trash stay there.
- `formats/`: Markdown and frontmatter (Obsidian syntax and inline images), transcripts (`Name  0:03`, `[0:03] Name: …`, `**Name**`, WebVTT, SRT, JSON segments), HTML, DOCX, ENEX, ProseMirror JSON, CSV. `files.rs` walks folders and zips. It recognises Obsidian, Notion, Bear, Logseq and Takeout layouts and reports what it leaves out (canvases, Bases, attachments other than images…).
- `providers/api.rs`: account imports with a personal key for Granola, Fireflies, Fathom, Otter (Enterprise), tl;dv, Circleback, MeetGeek, Jamie and Limitless. Each has its own pacing, and a 429 with `Retry-After` is honoured. Already-imported items are not fetched again, which spares rate limits.
- `providers/local.rs`: the Obsidian vault, the Anarlog/Hyprnote database, the Meetily database, and Apple Notes through `osascript` (JXA). Tables that hold another app's API keys are never opened.

Must not depend on tauri. Per-OS folders arrive in `KnownPaths` from the shell. Test with `cargo test -p cilica-import` (temporary folders, zips built in memory, recorded API shapes). To try it on what's really on this computer, writing into a throwaway database, run `cargo run -p cilica-import --example try_local -- obsidian`.
