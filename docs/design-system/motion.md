# Motion

Margin's motion is springy and physical, and always quick. Everything that moves animates; nothing makes you wait for it. Springs carry position, size and scale. Short tweens carry colour and opacity.

## Springs

Use these four presets in Motion (`transition={springs.snappy}`) and the matching `--spring-*` `linear()` curves in CSS.

| Preset | Motion config | CSS | Use for |
| --- | --- | --- | --- |
| snappy | `{ type: "spring", visualDuration: 0.2, bounce: 0.15 }` | `--spring-snappy` / `--spring-snappy-dur` | Press release, hover nudges, chevrons, checkbox, menus opening, icon motions |
| smooth | `{ type: "spring", visualDuration: 0.3, bounce: 0.1 }` | `--spring-smooth` | Panels and sidebar collapse, segmented thumb, active-row pill, palette and dialog open, diff card collapse |
| bouncy | `{ type: "spring", visualDuration: 0.35, bounce: 0.3 }` | `--spring-bouncy` | The record button morph, recorder capsule widening, Dynamic Island expand, toasts entering, drag-and-drop settling, switch knob |
| gentle | `{ type: "spring", visualDuration: 0.45, bounce: 0 }` | `--spring-gentle` | View changes: Notes ⇄ Transcript, opening a note, settings pages, onboarding steps |

Springs keep their velocity when interrupted, so a panel toggled twice reverses smoothly from where it is. Never block input while something animates.

## Tweens

| Token | Value | Use for |
| --- | --- | --- |
| `--dur-instant` | 80ms | Press feedback, menu item highlight |
| `--dur-fast` | 120ms | Hover colour and wash, tooltip in, all exits |
| `--dur-base` | 180ms | Fades, focus halo, scrim |
| `--dur-slow` | 280ms | Words arriving in a note or transcript |
| `--ease-out` | cubic-bezier(0.22, 1, 0.36, 1) | Every enter and hover |
| `--ease-in` | cubic-bezier(0.55, 0, 1, 0.45) | Exits only, and only at 120ms or less |

Linear timing is only for continuous things: progress, level meters, the shimmer and the countdown ring.

## Rules

1. **Enter on a spring, exit on a quick fade.** Exits run about 70% of the enter time and never bounce.
2. **Animate from where it came from.** Menus and popovers scale from their trigger (Radix transform-origin variables); toasts rise from the corner they live in; a note opened from the sidebar slides in from the left.
3. **One moving element for selection.** Active sidebar row, segmented thumb, palette highlight and tab indicators are a single element that travels (Motion `layoutId`), never a highlight that blinks off in one place and on in another.
4. **Lists reorder, they don't jump.** Use Motion `layout` on sidebar rows, toasts and transcript speakers so siblings slide when one is added, removed or dragged.
5. **Frequent actions stay fast.** Things you do dozens of times a minute (palette highlight, menu highlight, typing) use 80–140ms tweens with no bounce. Bounce is for moments: starting a recording, a drop landing, a toast arriving.
6. **Stagger sparingly.** Up to 6 items, 24ms apart, total under 150ms. Never stagger content you are trying to read.
7. **Transform and opacity only.** Animate `transform` and `opacity`; animate height through Motion `layout` or `grid-template-rows`. Never animate `backdrop-filter` blur radius: fade a pre-blurred layer instead. Apply `will-change` only while a gesture is active.
8. **Gestures follow the finger.** Dragging (sidebar rows, toasts, panel resize) tracks the pointer 1:1, then hands its velocity to a spring on release.
9. **Reduce motion** replaces travel with 120ms crossfades and keeps all feedback.

## Signature moments

- **Start recording**: the idle capsule's dot slides left, "Start recording" fades out, the timer and level meter fade in, the capsule's width springs (`bouncy`), the start sound plays, and the Dynamic Island grows out of the top edge 200ms later.
- **Stop recording**: Stop morphs to a check, the capsule shows "Writing notes…" in shimmer, the note's summary streams in word by word, then the notes-ready sound plays and the capsule returns to idle.
- **AI edit accepted**: the diff card lifts away, the changed sentence glows `accent-soft` and fades over 2s, and an Undo toast springs in.
- **Drop a note into a category**: the row lands with `bouncy`, the category count ticks up with a vertical number roll, and a macOS trackpad gives an alignment haptic.
- **Notes ⇄ Transcript (⌘E)**: the segmented thumb slides; the outgoing view fades and shifts 8px away; the incoming one slides in from the other side (`gentle`).
