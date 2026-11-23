# AssistantThread

A conversation with Cilica: full screen from Home, or in a sheet over a note.

- **Your messages**: right-aligned bubbles on `fill-press`, platform body text. iOS: radius 20 all round. Android: 20 with a 6 corner at the bottom right.
- **Cilica's replies**: no bubble. Note font (`note-body`), full width, Markdown rendered with the note styles. Streams word by word.
- **Tool steps**: one `ink-3` line each with a small icon ("Read 3 notes", "Searched transcripts for pricing"). Tap to expand the list of sources.
- **Citations**: inline chips with the note's name or a waveform and a timecode. Tapping opens the note at that point.
- **Edits**: when Cilica changes a note, a diff card (`bg-raised`, `radius-md`) shows strikes in `danger` and inserts on `accent-soft`, with Reject and Accept. Accept plays the success haptic and shows an Undo toast.
- **Actions under a reply**: Copy, Retry. No thumbs.
- **Working**: "Thinking…" in shimmer, and the composer's Send becomes Stop.

| | iOS | Android |
| --- | --- | --- |
| Scroll | `ScrollView` + `.scrollPosition`, sticks to the bottom while streaming unless you scroll up | `LazyColumn`, same rule |
| Jump to latest | Glass circle with ↓ above the composer | `SmallFloatingActionButton` |
| Copy | `UIPasteboard`, then a toast "Copied" | `ClipboardManager`; Android 13+ shows its own clipboard confirmation, so no toast |
