# Dialog

Dialogs ask for a decision that cannot be undone, or that blocks other work.

Opaque `bg-raised` (not glass), `radius-xl`, 420px, `shadow-overlay`, over a `scrim`. Title states the question; the body says exactly what will happen; the confirm button repeats the consequence ("Delete 3 notes"). Cancel is secondary and on the left. Esc and clicking the scrim cancel.

**Motion**: scale 0.96 → 1 with `--spring-smooth`; scrim fades `--dur-base`. Focus moves to Cancel for destructive dialogs, to the primary button otherwise.

Use dialogs only for the destructive tier. Everything reversible uses an Undo toast instead.
