# NoteEditor

The note page and its editing tools.

**Page**: title in `note-title` (Inter 28/34, 640; Newsreader under Serif notes), meta chips, then the body in `note-body` (17 on iOS, 16 on Android). Headings, lists, checklists, quotes, code and links use the desktop note styles at mobile sizes (see `tokens.json` › type › note). Text Cilica inserted keeps an `accent-soft` ground that fades over 2s.

**Editing**: tap anywhere in the body to edit; there is no separate edit mode. Notes save continuously.

**Format bar** (docked above the keyboard): Aa (styles) · checklist · bulleted list · link · attach · "Ask Cilica" on the right (acts on the selection, or the paragraph).

| | iOS | Android |
| --- | --- | --- |
| Editor | `TextEditor` with `AttributedString` | `BasicTextField` with a rich-text state |
| Format bar | Glass capsule, `ToolbarItemGroup(placement: .keyboard)` | `HorizontalFloatingToolbar` above the IME, `surfaceContainerHigh` |
| Aa | Replaces the keyboard with a format panel (styles row, B/I/U/S, lists, indent), as in Apple Notes | Opens a small bottom sheet with the same controls as a button group |
| Done | Glass ✓ in the nav bar; dismisses the keyboard | The keyboard's own dismiss; no Done button |
| Selection menu | Adds "Ask Cilica" and "Rewrite" | Adds "Ask Cilica" and "Rewrite" through the text toolbar |
