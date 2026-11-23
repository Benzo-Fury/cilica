# Chip

Small pills for metadata (date, people, folder) under a note title and for the composer's context.

| | iOS | Android |
| --- | --- | --- |
| Shape | Capsule, `fill-press`, 30pt | M3 `AssistChip`: 8dp corners, 1dp `outline`, 32dp |
| Text | `.footnote` medium, `ink-2` | `labelLarge`, `onSurfaceVariant` |
| Leading | SF Symbol or a speaker avatar (18pt, `speaker-N-soft` with a `speaker-N` ring) | Material Symbol or the same avatar |
| Selected (filters) | `accent-soft` ground, `ink` text | `FilterChip` selected: `secondaryContainer` with a check |

Tapping a chip opens its picker: date → calendar sheet, person → Saved People, folder → Move to folder.
