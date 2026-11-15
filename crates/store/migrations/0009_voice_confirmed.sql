-- A speaker teaches its person's voice only once someone confirmed who they are (docs/spec.md ›
-- Speakers and Saved people). Cilica's own guesses and the automatic "You" link are not
-- confirmations. Keeping the flag apart from the sample lets a speaker confirmed before their
-- voiceprint existed still teach once it does, and a sharper voiceprint later in the meeting
-- replace the sample it gave.
ALTER TABLE speakers ADD COLUMN voice_confirmed INTEGER NOT NULL DEFAULT 0;

-- Every speaker that already gave a sample was confirmed.
UPDATE speakers SET voice_confirmed = 1
  WHERE person_id IS NOT NULL AND id IN (SELECT speaker_id FROM voice_samples WHERE speaker_id IS NOT NULL);
