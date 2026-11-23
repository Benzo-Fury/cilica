# Transcript

The Transcript view of a meeting note, live or finished.

**Row**: speaker avatar (28, `speaker-N-soft` ground, 2px `speaker-N` ring, initial in `ink`) · name (`chip` style) · timecode (Geist Mono, `ink-3`, tabular) · the words in `transcript` (Inter 16/24).

- Consecutive turns by the same speaker merge under one header.
- Live: new words fade in over `dur-slow` and rise on `smooth`; the last line ends with a caret. When you scroll up, a round "Jump to live" button appears above the recorder bar.
- Tap a timecode to play from there (finished meetings). Long-press a line for Copy, Ask Cilica and Rename speaker.
- No coloured row backgrounds and no highlighting by speaker.

| | iOS | Android |
| --- | --- | --- |
| List | `LazyVStack` in a `ScrollView` with `.defaultScrollAnchor(.bottom)` while live | `LazyColumn` with `reverseLayout` while live |
| Jump to live | Glass circle with ↓ | `SmallFloatingActionButton` on `surfaceContainerHigh` |
