# Menu

Actions on a note, a row or a selection.

| | iOS | Android |
| --- | --- | --- |
| From a button (More) | `Menu`: system glass, icons trailing | `DropdownMenu`: `surfaceContainerHigh`, 16dp corners, icons leading |
| From a long-press on a row | `.contextMenu` with a note preview | `DropdownMenu` anchored to the row |
| Groups | `Section` (thick divider) | `HorizontalDivider` |
| Destructive | `Button(role: .destructive)`, last | `error`-coloured item, last |

Standard note menu: Pin · Move to folder · Share · Copy as Markdown · (divider) · Delete note.

Items are verbs. Never more than 8 items; group or move rarely used ones into a sheet.
