-- Voice details for Saved people (docs/spec.md › Speakers and Saved people).

-- Someone chose who this speaker is (picked a person, unlinked them or named them for one
-- meeting). Cilica's guesses never go over that choice, live or after the meeting.
ALTER TABLE speakers ADD COLUMN person_picked INTEGER NOT NULL DEFAULT 0;
UPDATE speakers SET person_picked = 1 WHERE voice_confirmed = 1;

-- How much speech a speaker's voiceprint was made from, and so how much each sample holds.
ALTER TABLE speakers ADD COLUMN voice_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE voice_samples ADD COLUMN speech_ms INTEGER NOT NULL DEFAULT 0;

-- The person Cilica last guessed a speaker was, kept after the person confirms or corrects it,
-- so a Saved person can show how often Cilica named them and how often it was corrected.
ALTER TABLE speakers ADD COLUMN guessed_person_id TEXT REFERENCES people(id) ON DELETE SET NULL;
UPDATE speakers SET guessed_person_id = person_id WHERE person_id IS NOT NULL AND person_picked = 0 AND is_self = 0;

-- Why this meeting's speakers could not be told apart: 'off' (speaker labels were off) or
-- 'unavailable' (the voice model could not be loaded). NULL when they were.
ALTER TABLE notes ADD COLUMN speaker_detection TEXT;
