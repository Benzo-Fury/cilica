# IconButton

Icon buttons are icon-only controls with no visible box until hover, following "icons without boxes".

**Use for** toolbar and sidebar actions that are recognisable without a label: new note, search, toggle sidebar, settings, copy, undo.

**Consumer provides**: one Lucide icon (18px, stroke 1.75) with its `nt-i-*` motion class, an `aria-label`, and a Tooltip with the label and shortcut.

**States**: rest `ink-2`; hover `fill-hover` wash and `ink`; pressed scale 0.92; toggled-on (`aria-pressed="true"`) icon in `accent-text`, no fill.

**Hit area** is 28px even though the glyph is 18px. Never smaller.

**Don't** use an icon button for anything destructive without a confirmation or Undo toast, or for an action whose icon is ambiguous (use a Button with a label).
