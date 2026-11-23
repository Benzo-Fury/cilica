# NoteRow

A note or meeting in a list (Notes, Meetings, Home's Recent, search results).

**Anatomy**: title (one line, ellipsis) · meta line: time or date, then the folder, or for meetings a waveform glyph and the duration. Optional: a snippet line in search results with the match in `ink` weight 600.

| | iOS | Android |
| --- | --- | --- |
| Component | `List` with `.listStyle(.plain)` on `bg-canvas` | `LazyColumn` of M3 `ListItem` |
| Height | 60pt minimum (two lines) | 72dp (M3 two-line) |
| Separators | 0.5pt `line`, inset to the text | None (M3) |
| Section headers | `.footnote` semibold `ink-3`, sentence case: Today, Yesterday, Previous 7 days, then month names | `titleSmall` `ink-3` |
| Press | `fill-press` wash | Ripple in `ink` at 10% |
| Swipe | Trailing: Delete (full swipe), Move. Leading: Pin | `SwipeToDismissBox`: end-to-start Delete with Undo snackbar; start-to-end Pin |
| Long-press | `.contextMenu` with a preview of the note | `DropdownMenu` anchored to the row, with a `LONG_PRESS` haptic |

- Pinned notes sit in a "Pinned" section at the top.
- A meeting whose notes are still being written shows "Writing notes…" in shimmer in place of the meta line.
