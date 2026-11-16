-- Each note's other names (its `aliases` property), one per line, so the notes list can carry them
-- and the editor can spot them as you type. Written with the body; existing notes are filled in by
-- the next link index refresh, which the reset below forces once.
ALTER TABLE notes ADD COLUMN aliases TEXT NOT NULL DEFAULT '';
UPDATE notes SET links_indexed = 0;
