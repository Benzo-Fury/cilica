-- The swear jar. Transcript swears are read from segments when the jar is counted; these rows are
-- the ones you typed into notes. A coin stays when its note goes: no refunds.
CREATE TABLE swear_coins (
  id       TEXT PRIMARY KEY NOT NULL,
  note_id  TEXT REFERENCES notes(id) ON DELETE SET NULL,
  -- The word's root ("fucking" is kept as "fuck").
  word     TEXT NOT NULL,
  at       TEXT NOT NULL
);
CREATE INDEX swear_coins_at ON swear_coins(at);

-- When the jar was emptied by hand. Only the newest row matters.
CREATE TABLE swear_jar_emptied (
  at TEXT NOT NULL
);
