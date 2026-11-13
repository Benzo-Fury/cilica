-- Imports no longer keep a history (docs/import.md). Each imported note remembers the item it came
-- from instead, so importing again skips what is already a note and brings back what was deleted,
-- undone or emptied from Trash.

ALTER TABLE notes ADD COLUMN import_provider TEXT;
ALTER TABLE notes ADD COLUMN import_id TEXT;
CREATE INDEX notes_import_source ON notes(import_provider, import_id) WHERE import_provider IS NOT NULL;

UPDATE notes SET
  import_provider = (SELECT provider FROM import_links WHERE note_id = notes.id ORDER BY imported_at DESC LIMIT 1),
  import_id = (SELECT external_id FROM import_links WHERE note_id = notes.id ORDER BY imported_at DESC LIMIT 1)
WHERE id IN (SELECT note_id FROM import_links);

DROP TABLE import_links;
DROP TABLE import_runs;
