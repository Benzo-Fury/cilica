# Button

| Variant | Use | iOS | Android |
| --- | --- | --- | --- |
| Primary | The one main action on a screen: New note, Stop, Save | `.buttonStyle(.glassProminent)` tinted `accent`, or a filled capsule outside glass | `Button` (filled), `primary` = `accent` |
| Secondary | Other actions with weight: Move, Share | `.buttonStyle(.glass)` or `fill-press` capsule | `FilledTonalButton` (`secondaryContainer`) |
| Outline | Low emphasis next to a primary | `.bordered` | `OutlinedButton` |
| Plain | Inline and toolbar text actions: Undo, Done | `.borderless` | `TextButton` |
| Destructive | Delete note | `role: .destructive` (red text) | `TextButton` in `error` |
| FAB | New note on list screens (Android only) | Not used: iOS has the bottom compose button | `ExtendedFloatingActionButton`, `primary`, collapses to an icon on scroll |

Sizes: iOS 44pt (52 large); Android 40dp (56 large). Text is the button style for the platform (17 semibold / 14 medium). Labels are verbs that name the outcome ("Delete note", never "OK").

Press: scale 0.97 released on `snappy` (both), plus the ripple on Android.
