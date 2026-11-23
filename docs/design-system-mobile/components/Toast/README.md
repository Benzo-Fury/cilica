# Toast

Brief confirmation of something that already happened, with at most one action (usually Undo).

| | iOS | Android |
| --- | --- | --- |
| Component | Custom glass capsule, top of the screen, under the Dynamic Island | M3 `Snackbar` via `SnackbarHost`, bottom, above the composer or FAB |
| Ground | `.glassEffect` | Inverse surface (`ink` ground, `bg-canvas` text), 16dp corners |
| Enter / exit | Drops in on `bouncy`; exits with a 120ms fade; swipe up to dismiss | M3 default; swipe to dismiss |
| Duration | 4s, or 8s with an action | `SnackbarDuration.Short` / `Long` with an action |

Copy: "Moved to Product", "Deleted note", "Notes ready · 42 min". Never "Success!". Errors that need a decision use a dialog instead.
