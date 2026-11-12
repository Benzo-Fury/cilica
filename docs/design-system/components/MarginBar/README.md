# MarginBar

Margin as a floating bar at the bottom of the page, chosen in Settings › Margin › Show Margin as (the default is the side panel). It replaces the right-hand panel and its toolbar button; `⌘J` opens and closes the bar instead.

**Surface**: solid `bg-raised`, `radius-xl`, a 0.5px `line-strong` hairline and `shadow-float`. Not glass. Only the movement borrows from Liquid Glass: one shape that stretches and settles.

**States**
- **Resting**: a 310×44 pill, 16px above the bottom edge, centred. Label only, no icon and no unread dot: "Ask Margin about this note" on a note, "Ask Margin about your notes" elsewhere, "Back to your conversation" once a conversation exists. The shortcut sits at the right.
- **Asking**: 560px wide, as tall as its content. Up to three suggestion chips above the composer. The caret is already in the field.
- **Conversation**: 560px wide, up to 60% of the window height (560px at most). Header with the title, Conversations, New conversation, Open in side panel and Collapse (the side panel has the matching "Show as floating bar" button); the thread; the composer.
- **No model yet** (bar and side panel alike): no illustration. A one-line amber notice above the composer says what is missing, with a Set up button that opens Settings › Margin; the composer is disabled.

**Collapsing**: Esc, the chevron, `⌘J` or a click on the page behind it. The conversation is kept.

**Motion**: width and height spring with `bouncy` when the pill opens into the composer and `smooth` for everything else; content fades in over `--dur-base`. Reduce motion uses the short crossfade.

**Placement rules**: hidden in Settings. Never wider than the page minus 32px. When a capsule is docked at the bottom (playback, the auto-stop countdown) the bar sits above it.

## Opening and closing

Both faces stay mounted and cross-fade. The open content is laid out at its final width and pinned to the bottom, so the growing shape reveals it without reflow. The shape springs (`smooth`) between measured pixel sizes, clamped to the room the page has (`openSize`), so nothing is cut off in a small window. With no conversation the bar has no header; "Open in side panel" sits beside the scope chip.
