-- Margin schema v1. Ids are UUID v7 as TEXT; timestamps are RFC 3339 UTC TEXT; booleans are INTEGER 0/1.
-- Add new migrations as 0002_*.sql; never edit a migration that has shipped.

CREATE TABLE categories (
  id          TEXT PRIMARY KEY NOT NULL,
  name        TEXT NOT NULL,
  position    REAL NOT NULL,
  collapsed   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE notes (
  id           TEXT PRIMARY KEY NOT NULL,
  title        TEXT NOT NULL DEFAULT '',
  body_md      TEXT NOT NULL DEFAULT '',
  snippet      TEXT NOT NULL DEFAULT '',
  category_id  TEXT REFERENCES categories(id) ON DELETE SET NULL,
  kind         TEXT NOT NULL DEFAULT 'manual' CHECK (kind IN ('manual','meeting')),
  meeting_app  TEXT,
  started_at   TEXT,
  ended_at     TEXT,
  position     REAL NOT NULL DEFAULT 0,
  -- Relative path of the recording under the app data dir, while retention keeps it.
  audio_path   TEXT,
  duration_ms  INTEGER,
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL,
  deleted_at   TEXT
);
CREATE INDEX notes_category ON notes(category_id, position);
CREATE INDEX notes_deleted ON notes(deleted_at);
CREATE INDEX notes_updated ON notes(updated_at DESC);

CREATE TABLE note_versions (
  id            TEXT PRIMARY KEY NOT NULL,
  note_id       TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  title         TEXT NOT NULL,
  body_md       TEXT NOT NULL,
  author        TEXT NOT NULL CHECK (author IN ('user','ai')),
  ai_action_id  TEXT,
  created_at    TEXT NOT NULL
);
CREATE INDEX note_versions_note ON note_versions(note_id, created_at DESC);

CREATE TABLE people (
  id            TEXT PRIMARY KEY NOT NULL,
  name          TEXT NOT NULL,
  color_index   INTEGER NOT NULL DEFAULT 1,
  -- Averaged, L2-normalised speaker embedding: little-endian f32 bytes.
  voiceprint    BLOB,
  sample_count  INTEGER NOT NULL DEFAULT 0,
  created_at    TEXT NOT NULL
);

CREATE TABLE speakers (
  id           TEXT PRIMARY KEY NOT NULL,
  note_id      TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  label        TEXT NOT NULL,
  person_id    TEXT REFERENCES people(id) ON DELETE SET NULL,
  color_index  INTEGER NOT NULL DEFAULT 1,
  is_self      INTEGER NOT NULL DEFAULT 0,
  -- Cluster centroid for this meeting: little-endian f32 bytes.
  embedding    BLOB
);
CREATE INDEX speakers_note ON speakers(note_id);

CREATE TABLE segments (
  id          TEXT PRIMARY KEY NOT NULL,
  note_id     TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  speaker_id  TEXT REFERENCES speakers(id) ON DELETE SET NULL,
  channel     TEXT NOT NULL CHECK (channel IN ('mic','system')),
  start_ms    INTEGER NOT NULL,
  end_ms      INTEGER NOT NULL,
  text        TEXT NOT NULL,
  is_final    INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX segments_note ON segments(note_id, start_ms);

CREATE TABLE tasks (
  id             TEXT PRIMARY KEY NOT NULL,
  name           TEXT NOT NULL,
  prompt         TEXT NOT NULL,
  scope_json     TEXT NOT NULL,
  schedule_json  TEXT NOT NULL,
  next_run_at    TEXT,
  enabled        INTEGER NOT NULL DEFAULT 1,
  is_default     INTEGER NOT NULL DEFAULT 0,
  created_at     TEXT NOT NULL
);

CREATE TABLE task_runs (
  id           TEXT PRIMARY KEY NOT NULL,
  task_id      TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  started_at   TEXT NOT NULL,
  finished_at  TEXT,
  status       TEXT NOT NULL CHECK (status IN ('running','succeeded','failed','cancelled')),
  summary      TEXT NOT NULL DEFAULT '',
  error        TEXT
);
CREATE INDEX task_runs_task ON task_runs(task_id, started_at DESC);

CREATE TABLE threads (
  id          TEXT PRIMARY KEY NOT NULL,
  title       TEXT NOT NULL DEFAULT '',
  -- Provider-side session id (Claude Code bridge --resume), if any.
  external_id TEXT,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);

CREATE TABLE messages (
  id          TEXT PRIMARY KEY NOT NULL,
  thread_id   TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
  turn_id     TEXT NOT NULL,
  role        TEXT NOT NULL CHECK (role IN ('user','assistant')),
  -- Vec<MessagePart> for display.
  parts_json  TEXT NOT NULL,
  -- The raw provider-format content (text + tool_use / tool_result blocks) used to rebuild the conversation.
  raw_json    TEXT,
  created_at  TEXT NOT NULL
);
CREATE INDEX messages_thread ON messages(thread_id, created_at);

CREATE TABLE proposals (
  id            TEXT PRIMARY KEY NOT NULL,
  source        TEXT NOT NULL CHECK (source IN ('chat','task')),
  thread_id     TEXT REFERENCES threads(id) ON DELETE SET NULL,
  task_run_id   TEXT REFERENCES task_runs(id) ON DELETE CASCADE,
  tier          TEXT NOT NULL,
  kind          TEXT NOT NULL,
  -- The serialised ProposalKind.
  payload_json  TEXT NOT NULL,
  reason        TEXT NOT NULL DEFAULT '',
  status        TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','applied','dismissed')),
  -- Stable hash of what was proposed, so a dismissed suggestion is not made again.
  fingerprint   TEXT,
  created_at    TEXT NOT NULL,
  resolved_at   TEXT
);
CREATE INDEX proposals_status ON proposals(status, created_at DESC);
CREATE INDEX proposals_fingerprint ON proposals(fingerprint);

CREATE TABLE ai_actions (
  id           TEXT PRIMARY KEY NOT NULL,
  thread_id    TEXT REFERENCES threads(id) ON DELETE SET NULL,
  turn_id      TEXT NOT NULL,
  tool         TEXT NOT NULL,
  tier         TEXT NOT NULL,
  args_json    TEXT NOT NULL,
  summary      TEXT NOT NULL DEFAULT '',
  status       TEXT NOT NULL,
  error        TEXT,
  -- How to reverse this action (null when it cannot be undone).
  undo_json    TEXT,
  proposal_id  TEXT REFERENCES proposals(id) ON DELETE SET NULL,
  created_at   TEXT NOT NULL
);
CREATE INDEX ai_actions_created ON ai_actions(created_at DESC);
CREATE INDEX ai_actions_turn ON ai_actions(turn_id);

CREATE TABLE settings (
  key         TEXT PRIMARY KEY NOT NULL,
  value_json  TEXT NOT NULL
);

-- Full-text search. One row per note; `transcript` is the concatenated final segments.
-- Kept in sync by the store (not by triggers) so transcript text can be batched.
CREATE VIRTUAL TABLE notes_fts USING fts5(
  note_id UNINDEXED,
  title,
  body_md,
  transcript,
  tokenize = 'unicode61 remove_diacritics 2'
);
