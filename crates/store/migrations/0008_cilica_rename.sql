-- The app was renamed from Margin to Cilica. Settings saved before the rename keep the assistant's
-- placement under its old key; move it to the new one so the choice survives.

UPDATE settings
SET value_json = json_remove(
  json_set(value_json, '$.appearance.cilica_placement', json_extract(value_json, '$.appearance.margin_placement')),
  '$.appearance.margin_placement'
)
WHERE key = 'settings' AND json_extract(value_json, '$.appearance.margin_placement') IS NOT NULL;
