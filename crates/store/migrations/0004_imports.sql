-- Imports from other apps (docs/import.md): one row per run, one link per imported note so a second
-- import of the same source only brings in what is new or changed.

CREATE TABLE import_runs (
  id            TEXT PRIMARY KEY NOT NULL,
  provider      TEXT NOT NULL,
  origin        TEXT NOT NULL DEFAULT '',
  status        TEXT NOT NULL CHECK (status IN ('running','succeeded','partial','failed','cancelled','undone')),
  imported      INTEGER NOT NULL DEFAULT 0,
  updated       INTEGER NOT NULL DEFAULT 0,
  skipped       INTEGER NOT NULL DEFAULT 0,
  failed        INTEGER NOT NULL DEFAULT 0,
  -- JSON array of strings: per-item problems, written for people.
  report_json   TEXT NOT NULL DEFAULT '[]',
  started_at    TEXT NOT NULL,
  finished_at   TEXT
);
CREATE INDEX import_runs_started ON import_runs(started_at DESC);

CREATE TABLE import_links (
  provider      TEXT NOT NULL,
  external_id   TEXT NOT NULL,
  note_id       TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  -- The run that created the note; undoing that run moves the note to Trash.
  run_id        TEXT NOT NULL REFERENCES import_runs(id),
  -- Hash of the item as read from the source, to tell whether it changed there.
  source_hash   TEXT NOT NULL,
  -- Hash of the title and body as written, to tell whether the person edited it in Margin.
  note_hash     TEXT NOT NULL,
  imported_at   TEXT NOT NULL,
  PRIMARY KEY (provider, external_id)
);
CREATE INDEX import_links_note ON import_links(note_id);
CREATE INDEX import_links_run ON import_links(run_id);

-- 0 when an imported transcript had no times: segment times then only keep the order.
ALTER TABLE notes ADD COLUMN transcript_timed INTEGER NOT NULL DEFAULT 1;
