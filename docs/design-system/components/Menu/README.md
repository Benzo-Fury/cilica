# Menu

Menus list actions for a selection: context menus, the More button, dropdowns.

**Layout**: min 220px, `radius-lg`, `glass-fill-strong`, 4px padding, 30px items with concentric `radius-lg - 4px` corners. Separators are 0.5px `line-strong`. Shortcuts align right.

**Motion**: opens from the pointer or trigger (`--origin`, Radix gives `--radix-dropdown-menu-content-transform-origin`), scaling 0.94 → 1 with `--spring-snappy`. Item highlight changes instantly (80ms): menus are read by moving through them fast.

Destructive items go last, after a separator, in `danger`.
