# Drawer

The app's map, modelled on the Claude app's drawer. It replaces a tab bar.

**Anatomy**: wordmark · search field · Home, Notes, Meetings · "Folders" section · "Recents" section (last 8 notes and threads) · footer with the avatar (opens Settings) and the "New note" pill.

| | iOS | Android |
| --- | --- | --- |
| Component | Custom: the stack slides right, `scrim` over the page | `ModalNavigationDrawer` + `ModalDrawerSheet` |
| Ground | `bg-sunken`, full height, no glass | `surfaceContainerLow`, trailing corners 16 |
| Width | 86% of the screen, max 360 | `min(360, width − 56)` |
| Selected item | `fill-press` wash, `ink` text | `primaryContainer` pill, `onPrimaryContainer` text, filled icon |
| Rows | Destinations 44pt with icon; folder and recent rows 38pt, text only | `NavigationDrawerItem` 56dp; text rows 48dp |
| Opening | ☰, or an edge pan on the root screen only | ☰, or a drag on the root screen; never from the left edge on deeper screens (that is back) |
| Tablet | `NavigationSplitView` sidebar | `PermanentNavigationDrawer` |

- Section headers use the section-header style (sentence case, `ink-3`).
- The only action is "New note" (`accent` pill). Everything else is a place.
- Long-press a folder or recent item for Rename, Pin and Delete.
