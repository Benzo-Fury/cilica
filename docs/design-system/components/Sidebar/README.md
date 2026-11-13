# Sidebar

**Current layout (supersedes the anatomy below where they differ).** Top to bottom:

1. **Title band**: traffic lights, then three icon buttons at the right: New category (`FolderPlus`), New note (`SquarePen`, a note with a pen), Hide sidebar.
2. **Search**: a field-shaped button showing `⌘K`. It opens the command palette, which searches notes and transcripts; the sidebar has no search of its own.
3. **Notes**: uncategorised notes first, with no heading. Then each category as a small `ink-3` heading (12px, weight 500) with a chevron after the name; a click folds it (200ms open, 160ms close, content fading with the height). Note rows are the title only; the date appears on the row you point at or focus; a recording note keeps its red dot. While a filed note is dragged, a drop zone appears at the top to take it out of its category.
4. **Bottom rows**: Scheduled tasks (a plain number badge for waiting proposals), Saved people, Settings. Margin activity and Trash are reached from Settings; Listening for meetings is in Settings › Recording.

Renaming a note or category edits the name in place, in the same type, with an amber underline that hugs the text (no input box).

Record is not in the sidebar: it lives in the note's title band.


The left sidebar holds navigation, categories and the bottom items. It sits on window material, never on canvas.

**Anatomy**
- Top: New note (secondary small button, ⌘N), New category icon button (⌘⇧N), Search icon button (⌘P).
- Fixed rows: All notes, Recording now (only while recording, with the breathing `rec` dot and a live timer).
- CATEGORIES (`caption` label) with collapsible rows; notes indent 34px under their category.
- Bottom: Saved people, Scheduled tasks (shows a proposal count chip), Trash, Settings (⌘,).

**Rows**: 32px tall, `radius-sm`, `ink-2` at rest, `fill-hover` on hover. The current row gets a `fill-press` pill and an `accent-text` icon; the pill is one shared element that slides between rows with `--spring-smooth` (Motion `layoutId`).

**Collapse**: chevron rotates 90° with `--spring-snappy`; children open and close with `--spring-smooth` on height plus a `--dur-base` fade.

**Drag and drop** (dnd-kit): the lifted row scales to 1.02, tilts -0.6°, takes `glass-fill-strong` and `shadow-float`. A 2px `accent-text` line with a ring marks the insert point; hovering a collapsed category for 500ms opens it and tints it `accent-soft`. On drop the row springs into place with `--spring-bouncy`, and a macOS trackpad gives an alignment haptic.

**Selection**: selected rows take an `accent-soft` ground (`Row selected`), from the first `⌘`-click; the open note keeps its `fill-press` pill. Group actions live in the row's context menu only (no toolbar): "Move 3 notes to", "Ask Margin about 3 notes", then "Move 3 notes to Trash" after a separator. Clicking outside the note rows clears the selection. Dragging a selected row lifts the whole selection: the preview reads "3 notes" and the other selected rows dim.

**Width**: 248px default, resizable 200 to 360, snaps back to 248 within 12px. ⌘\ collapses it with `--spring-smooth` (content slides, it does not fade).
