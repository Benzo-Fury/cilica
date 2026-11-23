# Sheet

| Use | iOS | Android |
| --- | --- | --- |
| Cilica from a note | `.sheet` with `[.medium, .large]` detents; background interaction enabled up to medium, so the note behind still scrolls | `ModalBottomSheet`, partially expanded, draggable to full |
| Pickers (Move to folder, Note font, Speaker) | `.medium` detent | `ModalBottomSheet` |
| Settings | Full-height sheet with ✕ | Not a sheet: a full-screen destination |
| Share to Cilica | Share extension, compact | `ModalBottomSheet` from the share target |

- iOS sheets are system glass at partial height and become opaque at full height. Don't paint over them.
- Android sheets are `surfaceContainerHigh` (`bg-raised`) with 28dp top corners and a drag handle, over the `scrim`.
- Sheet titles: iOS `.headline` centred in a toolbar; Android `titleMedium`, left-aligned.
- Predictive back (Android) and swipe-down (both) dismiss. A sheet with unsaved input asks before closing.
