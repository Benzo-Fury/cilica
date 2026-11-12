-- The link index (docs/spec.md › Links and the graph): [[wiki links]], ![[embeds]], Markdown links to
-- notes and #tags, read from each note's body. Kept cheap: saving a note only flips `links_indexed`
-- (the trigger below); the next graph or backlinks read re-reads just the notes marked stale.

CREATE TABLE note_links (
  note_id     TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  kind        TEXT NOT NULL CHECK (kind IN ('link','embed','tag')),
  -- Lowercased with whitespace folded: a note title for links and embeds, the tag for tags.
  target_key  TEXT NOT NULL,
  -- As first written, to name a note that is not created yet or show a tag.
  target      TEXT NOT NULL,
  PRIMARY KEY (note_id, kind, target_key)
) WITHOUT ROWID;
CREATE INDEX note_links_target ON note_links(target_key);

ALTER TABLE notes ADD COLUMN links_indexed INTEGER NOT NULL DEFAULT 0;
CREATE INDEX notes_links_stale ON notes(id) WHERE links_indexed = 0;

CREATE TRIGGER notes_links_stale AFTER UPDATE OF body_md ON notes
WHEN NEW.body_md IS NOT OLD.body_md
BEGIN
  UPDATE notes SET links_indexed = 0 WHERE id = NEW.id;
END;
