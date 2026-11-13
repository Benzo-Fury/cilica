# ThemeChoice

The theme picker in Settings › Appearance: a radio group of cards (`ThemeChoices`, `ThemeChoice` in `src/ui/ThemeChoice.tsx`), one per theme in `src/app/theme/themes.ts`. Picking a card applies the theme at once.

**The card is the preview.** Each card is a 16:10 miniature window carrying its theme's `data-theme` and `data-look`, so every colour in it comes from that theme's own tokens: a sidebar strip on `bg-sunken` with one `accent-soft` row, a title in `ink`, three lines in `ink-4` and the recorder capsule in `accent`. A scene theme paints its scene (`SceneArt`, still, one scene pixel per CSS pixel) under the same veil and panel wash as the real window. System shows Light on the left half and Dark on the right.

**States.** Rest: a 0.5px `line-strong` edge. Hover: `shadow-sm`. Press: scale 0.97, released on `--spring-snappy`. Selected: a 2px `bg-raised` gap and a 2px `accent-text` ring, with the name in `ink`. Keyboard focus: the `--focus` ring on the preview. Names sit under the card in `label` size, sentence case.

**Layout.** `repeat(auto-fill, minmax(136px, 1fr))` with `space-3` gaps, under the Theme row's description.
