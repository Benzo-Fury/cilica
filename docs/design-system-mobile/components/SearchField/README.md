# SearchField

| | iOS | Android |
| --- | --- | --- |
| Lists (Notes, Meetings) | `.searchable` in the bottom toolbar: a glass pill with the compose button beside it; minimises to a glass circle while scrolling | M3 `SearchBar` under the top app bar, `surfaceContainer`, 56dp, full pill |
| Drawer | A plain field on `fill-press`, 38pt, radius 12 | The same M3 search bar shape, inside the drawer |
| Active | Moves above the keyboard; results replace the list | `ExpandedFullScreenSearchBar` with recent searches; predictive back closes it |
| Placeholder | "Search" | "Search notes" |

Results show note rows with the matched snippet. Searching is local (the Rust core's full-text index), so results update on every keystroke with no spinner.
