# Checkbox

Checkboxes mark tasks done in notes and pick items in lists.

**Consumer provides**: `role="checkbox"`, `aria-checked`, the item text next to it.

**Motion**: fill fades to `accent` and the tick draws itself in over `--dur-base` (stroke-dashoffset), 40ms after the fill. Press scales to 0.88.

Completed tasks in a note turn `ink-3` with a `ink-4` strike-through; they never jump to the bottom of the list on their own.
