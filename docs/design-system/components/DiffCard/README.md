# DiffCard

Diff cards show an AI edit to an existing note before it is applied.

**Anatomy**: header (edit icon, "Edit", the note and section, "1 of 2"), the proposed prose with insertions on `accent-soft` and deletions in `danger` with strike-through on `danger-soft`, and a footer with the tier and Reject (⌘⌫) / Accept (⌘↩).

**Tiers** (from the product spec): reads run freely; additive changes apply with an Undo toast; edits always come as diff cards; destructive actions always ask in a Dialog.

**Accept**: the card fades and lifts 4px with `--spring-smooth`, the note's changed text gets an `accent-soft` wash that fades over 2s, and a toast offers Undo.
**Reject**: the card collapses; the assistant replies "Left it unchanged".

Show the whole sentence around a change, never a lone word.
