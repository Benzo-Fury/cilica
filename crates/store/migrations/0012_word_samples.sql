-- Voice samples Cilica learnt from words someone moved to a speaker by hand. They point at the
-- line those words became, so undoing the move takes the sample back. NULL for samples made
-- from a speaker's whole voiceprint.
ALTER TABLE voice_samples ADD COLUMN segment_id TEXT;
CREATE INDEX voice_samples_segment ON voice_samples(segment_id);
