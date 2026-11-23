# AppBar

The bar at the top of every screen. It holds navigation on the left and up to two actions on the right.

| | iOS | Android |
| --- | --- | --- |
| Component | `.toolbar` on a `NavigationStack` | `TopAppBar` / `LargeTopAppBar` |
| Leading | ☰ (root) or back, as a glass circle | ☰ or ← as a 48dp icon button |
| Title | Large title on lists (`.large`), inline on notes; the note title only appears once it scrolls away | Left-aligned `titleLarge`; `LargeTopAppBar` collapses on scroll |
| Actions | Up to 2, grouped into one glass capsule when there are two | Up to 2 icons, then `more_vert` overflow |
| More icon | `ellipsis` (horizontal) | `more_vert` (vertical) |
| Ground | Transparent over `bg-canvas`; the system adds a scroll-edge effect | `surface` (`bg-canvas`); `surfaceContainer` when scrolled under |

Rules:
- One primary action at most. "New note" is never in the bar on list screens (it is the bottom compose button or the FAB).
- Don't put text buttons in the bar except "Done" while editing (iOS: a glass checkmark).
