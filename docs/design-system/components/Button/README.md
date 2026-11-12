# Button

Buttons trigger one action; the label is the verb that happens.

**Variants**
- `nt-btn-primary`: one per surface, the action most people take next (Start recording, Accept, Continue). Flat `accent` fill with a 0.5px darker edge, `on-accent` text at weight 500 (other buttons are 550). No inner highlight and no sheen.
- `nt-btn-secondary`: neutral alternatives (New note, Cancel). `bg-raised` with a `line-strong` hairline.
- `nt-btn-ghost`: low-emphasis actions in toolbars and toasts (Share, Undo).
- `nt-btn-glass`: only on controls that float over content or the desktop (recorder, floating toolbars). Never on an opaque surface.
- `nt-btn-danger`: destructive. Soft at rest, fills `danger` on hover. In a confirmation dialog it starts filled.

**Sizes**: `nt-btn-sm` 28px (toolbars, cards), default 32px, `nt-btn-lg` 40px (dialogs, onboarding).

**Consumer provides**: label text (sentence case, a verb), an optional leading Lucide icon with an `nt-i-*` motion class, an optional trailing Kbd, `disabled`.

**Motion**
- Hover: colour fades in `--dur-fast`; secondary lifts 1px and gains `shadow-sm`; primary steps to `accent-hover`; glass shows a specular highlight that follows the pointer (`--mx`/`--my`).
- The icon plays its one-shot animation on hover and focus (plus rotates, arrow nudges, trash tips).
- Press: scale 0.97 in `--dur-instant`; release springs back with `--spring-snappy`.

**Do**: name the outcome ("Delete 3 notes", not "OK"). Show the shortcut on primary actions in dialogs and cards.
**Don't**: put two primary buttons side by side, use glass on `bg-canvas`, or animate width on label change.
