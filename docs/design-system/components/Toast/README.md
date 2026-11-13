# Toast

Toasts confirm what just happened and offer Undo. Cilica's own toast library (`src/lib/toast`) renders them.

**Placement**: bottom-right, 16px from the edges, `z-toast`, 356px wide. On narrow windows, bottom-centre above the recorder capsule.

**Look**: `glass-fill-strong`, `radius-lg`, `shadow-float`. Icon, title (`headline`), optional description (`callout`), one action.

**Motion**: enters from 24px below at 0.9 scale with `--spring-bouncy`; a title-only toast first appears as a pill and grows into a card when it has a description. Older toasts stack behind, 8px up and 4% smaller each, max 3 visible; hover fans the stack out. Exit: fade and 8px drop in `--dur-fast`. Swipe right to dismiss.

**Undo toasts** show a 5s countdown ring in `accent-text` and pause it while hovered. ⌘Z works while any Undo toast is visible.

**API**: `toast()`, `.success`, `.error`, `.action`, `.undo`, `.promise`. Titles state the result in past tense ("Moved 3 notes to Trash"), never "Success!".
