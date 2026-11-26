-- Text Cilica read in note images on this device (Settings › Writing › Find text in images).
-- One row per image file, by content hash, so the same image in many notes is read once. Empty
-- `text` means the image was read and has no text; `error` is set when it couldn't be read. Either
-- way it isn't read again.
CREATE TABLE attachment_text (
  hash           TEXT PRIMARY KEY NOT NULL,
  text           TEXT NOT NULL,
  error          TEXT,
  recognized_at  TEXT NOT NULL
);

-- Search over image text. One row per image with text that a note points at; `path` is the
-- reference as written in the note (`attachments/<hash>.<ext>`). Kept in sync by the store when a
-- note body is written and when an image's text is recorded.
CREATE VIRTUAL TABLE image_text_fts USING fts5(
  note_id UNINDEXED,
  path UNINDEXED,
  text,
  tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER notes_image_text_deleted AFTER DELETE ON notes
BEGIN
  DELETE FROM image_text_fts WHERE note_id = OLD.id;
END;
