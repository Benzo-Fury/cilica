# Dialog

Confirmation for actions that cannot be undone, and errors that need a decision. Prefer Undo toasts for everything else.

| | iOS | Android |
| --- | --- | --- |
| Component | `.alert` (or `.confirmationDialog` for choices from a button) | `AlertDialog` |
| Buttons | Cancel and the action side by side; destructive in red | Right-aligned text buttons; the action last |
| Title | States the action: "Delete 3 notes?" | Same, `headlineSmall` |
| Body | The consequence: "This can't be undone." | Same |

Button labels repeat the action ("Delete notes"), never "OK" or "Yes".
