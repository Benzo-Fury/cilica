# margin-agent

The assistant and the only write path for AI.

- `tools`: the registry (spec tool table), with JSON schemas and base tiers. `runner::ToolRunner` runs calls and enforces the guardrails in code: Read runs; Additive applies with undo data; Edit becomes a diff `Proposal` unless `auto_apply_edits`; Destructive always proposes; a write that takes the turn past 5 distinct notes proposes; at most 20 writes per turn; propose-only mode for tasks with fingerprint de-duplication. Every call writes an `ai_actions` row and emits events.
- `patch`: `edit_note` takes exact search/replace edits; proposals rebase onto later user changes or are rejected as conflicts.
- `ops`: apply (snapshotting AI versions first) and undo for every write.
- `Agent`: threads, turns (streaming, parallel tools, cancellation, history rebuilt from raw provider content, trimming, titles, Undo toast), proposals, undo (one action or a turn), prompt chips, task runs, and `Summarizer` (`MeetingSummarizer`).
- `store::NoteStore`: the seam to the notes/transcripts store, with `DbNoteStore` (real) and `FakeNotes` (in memory).

Note and transcript content always reaches the model as quoted data (`prompt::quoted`).

Must not depend on tauri. Test with `cargo test -p margin-agent` (MockProvider + FakeNotes + in-memory Db; one test runs against the real store).
