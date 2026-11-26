# ImageBlock and ImageViewer

An image in a note, and the full-window viewer it opens into. Code: `src/ui/ImageBlock.tsx`, `src/ui/ImageViewer.tsx`, styles in `src/ui/images.css` (mirrored at the end of `bundle.css`). Behaviour and storage: spec, "Images in notes".

## ImageBlock

- **At rest** the picture sits at its width (or its own size, never wider than the column), placed left (default), centred or right, with `radius-sm` corners and `space-3` below. No frame, border or shadow: the image is content.
- **Pointer**: a transparent layer covers the picture and takes the pointer (and the double-click that opens the viewer), so macOS Live Text never reads the image and never draws its own selection over the words in a selected picture.
- **Hit area**: only the picture (with its grip and resize edges) and the caption take the pointer. The empty column beside a narrow image and the gap under it pass clicks to the note, so clicking there deselects the image like clicking anywhere else.
- **Hover** shows the grip (`GripVertical`, 16px, `ink-4`) in the left margin, `space-8` out from the image. Drag it to move the image; ↑ / ↓ on the focused grip move it a block.
- **Selected** (while the note or its toolbar has focus; clicking anywhere else, a sidebar included, clears it): no ring or border. The picture lifts: `translateY(-2px) scale(1.01)` on `spring-snappy`, with `shadow-float` fading in underneath. Its whole edge becomes a resize area: invisible grab zones 16px deep on each side and 24 × 24px on each corner, half outside the picture and half over it, with the matching resize cursor (`ns`, `ew`, `nesw`, `nwse`). Any of them keeps the shape. While dragging, a small `bg-raised` chip in the top right shows the width in tabular px.
- **Loading**: a 4:3 skeleton (the shimmering `nt-skel`) at the image's width; the picture fades in over it (`--dur-base`). Being saved or downloaded: 60% opacity.
- **Moving**: the image waits at 35% opacity where it was; a 2px `accent-text` pill line marks where it will land (it springs in with `--spring-snappy`). On drop it settles from 0.96 scale with `--spring-bouncy`.
- **Caption**: optional, under the picture, 13/18 `ink-3`, aligned with the picture (centred under a centred image). Edited in place; the placeholder is "Add a caption".
- **Missing**: a card in the image's place on `bg-sunken` with a 0.5px `line` edge and `radius-md`: `ImageOff`, a title ("Image missing", "Web image not shown"), one line saying why, and small buttons (Replace…, Remove; Save a copy).
- **Reduce motion**: no settle or line spring; a selected image keeps its shadow but doesn't lift.

## Image toolbar and menu

The selection toolbar's bar (`FormatBar` on strong glass), placed under the image (above when there is no room): Align left / Centre / Align right (`AlignHorizontalJustify*`, pressed for the current one), Captions, Alt text (`TextCursorInput`, swaps the buttons for a field), View image (`Maximize2`), and ⋯ (Copy image, Save image as…, Show in Finder, Replace…, then Delete image in danger red after a separator). Right-click gives the same as a context menu.

## ImageViewer

- Over a scrim of the window's own colour at 92% with a 12px blur (solid under Reduce transparency), so it follows the theme.
- The picture is fitted with `space-12` around it, `radius-sm` and `shadow-overlay`. It opens with `springs.smooth` from 0.96 scale; exits with the quick exit tween; images crossfade as you step. Reduce motion: crossfade only.
- A strong-glass bar under the picture, `radius-lg`, `shadow-float`: previous / "2 of 5" / next (only with more than one image), zoom out, fit, zoom in, close. Tooltips show the keys (← → − 0 + Esc).
- Zooming (pinch, ⌘-scroll, double-click, + / −) keeps the point under the pointer still; zoom and fit animate with `springs.snappy`, pinch and pan follow the fingers directly. Up to 8×.
- The caption, if any, under the bar in 13/18 `ink-2`.
