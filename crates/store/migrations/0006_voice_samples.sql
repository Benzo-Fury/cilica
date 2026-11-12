-- Saved voices keep each sample (docs/spec.md › Speakers and Saved people): one embedding per
-- speaker the person confirmed, so a correction can take a wrong sample back out. The person's
-- `voiceprint` stays as the normalised mean of their samples and `sample_count` as their number.
CREATE TABLE voice_samples (
  id          TEXT PRIMARY KEY NOT NULL,
  person_id   TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  -- The meeting speaker it came from; NULL for samples from before samples were kept.
  speaker_id  TEXT REFERENCES speakers(id) ON DELETE SET NULL,
  -- L2-normalised embedding: little-endian f32 bytes.
  embedding   BLOB NOT NULL,
  created_at  TEXT NOT NULL
);
CREATE INDEX voice_samples_person ON voice_samples(person_id);
CREATE INDEX voice_samples_speaker ON voice_samples(speaker_id);

-- Each existing averaged voiceprint becomes that person's first sample.
INSERT INTO voice_samples (id, person_id, speaker_id, embedding, created_at)
  SELECT lower(hex(randomblob(16))), id, NULL, voiceprint, created_at FROM people WHERE voiceprint IS NOT NULL;
UPDATE people SET sample_count = 1 WHERE voiceprint IS NOT NULL AND sample_count > 1;

-- "You": the app's user, a Saved person like any other that is always there.
ALTER TABLE people ADD COLUMN is_self INTEGER NOT NULL DEFAULT 0;
CREATE UNIQUE INDEX people_one_self ON people(is_self) WHERE is_self = 1;
