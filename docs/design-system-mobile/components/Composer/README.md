# Composer

Where you ask Cilica. It is docked at the bottom of Home, of threads and (as a pill) of a note.

**Anatomy**: a multiline field (grows to 8 lines, then scrolls), then an action row with attach (+), the context chip ("All notes", "This note", a folder) and the round accent button.

**The round button**
- On Home with an empty field it is **Record** (a `rec` dot ringed in `accent`). Tapping it starts a recording and the composer morphs into the recorder bar.
- As soon as there is text it morphs into **Send** (↑) with `bouncy`. Clearing the text morphs it back.
- Everywhere else it is always Send, dimmed (40%) while empty.
- While Cilica answers, it is **Stop** (`ink` fill, square glyph).

| | iOS | Android |
| --- | --- | --- |
| Material | `.glassEffect(.regular, in: .rect(cornerRadius: 26))` | `surfaceContainer` (`bg-sunken` light, `bg-raised` dark), 28dp corners, level 2 shadow |
| Round button | 36pt, `.glassProminent` tinted `accent` | 40dp `FilledIconButton`; Record is a rounded square (shape morph to circle for Send) |
| Text | `.callout` / 16 | `bodyLarge` / 16 |
| Keyboard | Sits on top of the keyboard (`safeAreaInset(edge: .bottom)`) | `imePadding()` |
| Collapsed pill (notes) | Capsule with placeholder and Send, expands to the full composer on focus | Same |

Placeholders: "Ask Cilica" (Home), "Ask about this note" (a note), "Reply to Cilica" (a thread).
