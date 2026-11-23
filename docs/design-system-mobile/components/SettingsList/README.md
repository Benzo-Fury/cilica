# SettingsList

Grouped rows for Settings and any other preferences page.

**Row**: leading icon (`ink-2`) · label · trailing value (`ink-3`), switch or chevron. Optional description under the label. Groups have a sentence-case header and an optional footer (privacy copy goes here: "Audio stays on this iPhone").

| | iOS | Android |
| --- | --- | --- |
| Component | `Form` / `List(.insetGrouped)` on `bg-sunken`, cells on `bg-raised` | `ListItem`s in rounded `surfaceContainerHigh` groups on `surfaceContainerLow` (Android 16 Settings style) |
| Row height | 44pt | 56dp (72 with a description) |
| Switch | `Toggle` tinted `accent` | M3 `Switch`, `primary` = `accent` |
| Navigation rows | Chevron, push | No chevron, push |
| Appearance | Three cards: Light, Dark, System (selected card gets a 2pt `accent` ring) | A Theme row that opens a single-choice dialog |

**Sections (v1)**: Account · Recording (Transcription engine, Start and stop sounds, Haptic feedback) · Cilica (Model, API key) · Notes (Note font, Default folder) · Appearance · Privacy · About.

The "AI" section on desktop is "Cilica" here, except the provider and key rows, which say what they are.
