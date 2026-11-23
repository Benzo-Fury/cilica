# RecorderBar

Shown at the bottom of a note while it records, in place of the composer.

**Anatomy**: `rec` dot (breathing, 2.4s) · timer (Geist Mono, tabular) · level meter (live bars, `ink-3`) · Pause · **Stop** (`accent` pill).

**States**
1. **Recording**: as above.
2. **Paused**: the dot stops breathing and turns `ink-3`; Pause becomes Resume; the timer freezes.
3. **Writing notes**: Stop morphs into a ✓, then the bar shows "Writing notes…" in shimmer while the summary streams into the note.
4. **Done**: the bar shrinks back into the composer (`bouncy`) and the success haptic plays.

| | iOS | Android |
| --- | --- | --- |
| Material | `.glassEffect(.regular, in: .capsule)`, shares a `glassEffectID` with the composer's Record button so one morphs into the other | `surfaceContainerHigh` pill, level 2, 64dp |
| Height | 56pt | 64dp |
| Stop | `.glassProminent`, `accent` tint | Filled button, `primary`; the record button's shape morphs from rounded square to circle |
| Background | Live Activity + Dynamic Island (see RecordingActivity) | Foreground service notification (see RecordingActivity) |

Accessibility: the bar is one element reading "Recording, 12 minutes 14 seconds", with Pause and Stop as actions.
