# SegmentedControl

Switches between two or three views of the same thing, most often Notes ⇄ Transcript on a meeting note.

| | iOS | Android |
| --- | --- | --- |
| Component | `Picker` with `.pickerStyle(.segmented)` (glass in iOS 26) | `SingleChoiceSegmentedButtonRow` |
| Selected | Raised thumb that slides (`smooth`) | `secondaryContainer` fill with a check icon |
| Haptic | `.selection` | `SEGMENT_FREQUENT_TICK` |
| Placement | Under the nav bar, above the title | Same |

The content change uses `gentle`: the outgoing view fades and shifts 8 away, the incoming one slides in from the other side. Never more than 3 segments; use a menu beyond that.
