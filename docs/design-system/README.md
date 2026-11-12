Margin is a private, local-first desktop app that records meetings without a bot, transcribes them on-device and turns them into Markdown notes with an AI assistant beside them. It runs on macOS, Windows and Linux (Tauri 2, React 19). The interface should feel like a first-party Apple app on every platform: one calm opaque sheet, glass only on controls that float, one amber accent, and quick, physical motion on everything that moves.

## Principles

1. **The shell is solid, floating controls are glass.** The sidebar sits on `bg-sunken`; notes, transcripts and the assistant sit on `bg-canvas`, split by hairlines. Glass is only for controls that float: the recorder, menus, toasts, the palette. Never put reading text on glass thinner than `glass-fill-strong`.
2. **Monochrome first, one accent.** Neutrals carry the whole layout. `accent` marks the single most important control on a surface and the current selection. Red (`rec`) means recording and nothing else.
3. **Everything that moves, springs.** Every change of position, size or state animates, fast enough that it never makes you wait (most settle in under 350ms). See Motion.
4. **Quiet, with small delights.** No mascots, confetti or colour for its own sake. Delight lives in details: icons that react to hover, the record button morph, a tick that draws itself.
5. **Comfortable density.** 32px rows, 14px UI text, hit areas never under 28px. Roomier than Linear, tighter than a consumer app.
6. **Same app on every OS.** One bundled typeface, one icon set, Margin's own window chrome on Windows and Linux. Each OS still gets its own shortcut glyphs.

## Voice and copy

- Speak plainly, in the second person, sentence case everywhere ("Start recording", "Saved people").
- Buttons are verbs that name the outcome: "Delete 3 notes", "Accept", "Move to Trash". Never "OK", "Submit" or "Yes".
- Toasts report what happened in past tense: "Moved 3 notes to Trash", "Notes ready". Never "Success!".
- Errors say what went wrong and what to do next: "That key was rejected by Anthropic. Check it and paste it again." No apologies, no error codes in the title.
- The assistant is "Margin" in copy, never "AI" or "the bot". The one exception is the "AI" section in Settings. It replies in plain text, cites the note and timecode it used, and never uses emoji.
- Privacy copy is concrete: "Audio stays on this Mac", "No bot joins the call".
- No emoji anywhere in the UI. Numbers use digits ("3 notes", "42 min").

## Color

Two themes, designed together and following the OS setting (Light, Dark or System in Settings › Appearance).

- **Grounds**: `bg-window` (opaque window, shown where vibrancy is unavailable), `bg-canvas` (the note page), `bg-raised` (cards, dialogs, diff cards), `bg-sunken` (inputs, code). Washes for interaction are `fill-hover` and `fill-press`; edges are `line` and `line-strong`.
- **Text**: `ink` for primary text, `ink-2` secondary, `ink-3` tertiary (timestamps, counts, kbd), `ink-4` only for placeholders and disabled labels. `ink`, `ink-2` and `ink-3` all meet 4.5:1 on every ground and glass fill in both themes.
- **Accent**: `accent` is the fill (primary button, switch on, send). Text on it is always `on-accent`, a dark brown, in both themes. `accent-text` is the accent as text, link, selected icon and focus ring. `accent-soft` is the tinted ground for selection and AI-inserted text.
- **Primary buttons are amber.** This follows Apple's guidance that the one prominent button in a view carries the app's accent colour (macOS default buttons, Liquid Glass prominent buttons), as Linear, Notion and Granola also do. Keep it to one amber button per surface so it keeps its meaning; everything else is secondary, ghost or glass.
- **Recording**: `rec` and `rec-soft` are reserved for recording state.
- **Status**: `success`, `danger`, `warning`, `info` each with a `-soft` ground. Status is never colour alone: success has a check, warning a triangle, danger an icon or strike-through. Success is a blue-leaning teal so it never relies on red/green alone.
- **Speakers**: `speaker-1` … `speaker-8` (ring, dot, waveform) with `speaker-N-soft` grounds behind `ink` initials. A Saved Person keeps the colour they choose.

**Dark theme ground.** Margin's dark grounds are warm graphite (`bg-canvas` #1b1917, `bg-window` #131110), not pure black. Pure white text on #000 causes halation for readers with astigmatism; shadows and glass depth disappear on black; and Material Design, macOS and most premium tools settle on an 8–14% grey. The faint warm bias (hue 70) keeps the neutrals in the same family as the amber accent. Text is off-white `ink` (#ebe9e7), never #fff.

**Accent presets.** Settings › Appearance offers Amber (default), Lime, Mint, Sky, Indigo, Violet, Rose and Graphite (`preset-*` swatches). Picking one regenerates four tokens in OKLCH from its hue `h`, keeping lightness and chroma: `accent` = oklch(0.76 0.165 h) light / oklch(0.77 0.155 h) dark; `accent-hover` 0.04 darker in light, lighter in dark; `accent-text` = oklch(0.52 0.14 h) light / oklch(0.81 0.13 h) dark; `accent-soft` = oklch(0.955 0.035 h) light / oklch(0.295 0.055 h) dark. Re-check `on-accent` contrast; for Graphite, `on-accent` becomes `bg-canvas`.

**Focus ring.** `--focus`: a 2px `bg-canvas` gap, then a solid 2px `accent-text` ring. Passes 3:1 on every surface in both themes. Shown on keyboard focus only (`:focus-visible`).

## Typography

One bundled family on every OS: **Inter** (variable, with the optical-size axis), shipped as a woff2 inside the app; Margin never falls back to the system font. Inter's optical sizes do what SF Pro Text and Display do on a Mac: tighter, crisper letterforms at 28px, more open ones at 12px. Turn on `font-optical-sizing: auto` and the `cv05` (tailed l) and `cv08` features. **Newsreader** is the optional serif note font and **Geist Mono** sets timecodes, inline code and the Mono note font. All three are open-licensed (OFL).

- Interface: `display` (note title), `title` (dialogs, settings pages), `headline` (panel and card titles), `body` (default UI), `callout` (secondary UI), `label` (chips, tooltips), `caption` (uppercase sidebar section labels).
- Note body: `note-h1`, `note-h2`, `note-h3`, `note-body` (Sans, default), `note-body-serif`, `note-body-mono`, `transcript`.
- Data: `timecode` (always tabular), `kbd`.

Letter-spacing tightens as size grows (-0.022em at 28px, 0 at 12px, +0.02em on uppercase captions). Headings get `text-wrap: balance`. Every number that updates live (timers, counts) uses tabular figures.

## Layout and density

- Window: left sidebar (`size-sidebar` 248px) and right assistant (`size-assistant` 380px) sit flush against the note: sidebar on `bg-sunken`, note and assistant on `bg-canvas`, split by `line-strong` hairlines, with no card, radius or shadow.
- Title band: `size-titlebar` 52px, shared by the traffic lights (or Margin's own window buttons), the sidebar toggle, the note title and the Notes / Transcript segmented control.
- Note column: `size-measure` 720px max, centred, `space-12` top padding, `space-16` bottom runway for the recorder capsule.
- 4px spacing grid (`space-*`). Group siblings with `gap`, not margins.
- Rows and controls: `size-row` and `size-control` 32px, `size-control-sm` 28px, `size-control-lg` 40px. Icon glyphs 18px (`size-icon`), 16px in chips and menus.

## Shape

Every rounded rectangle is a **squircle** (continuous corner), as on Apple platforms. Where the WebView supports CSS `corner-shape: squircle` (Chromium 139+, so WebView2 on Windows), bundle.css applies it and multiplies the radius by `--sq` (1.45) so corners keep their visual size. Where it is unsupported (macOS WKWebView and Linux WebKitGTK today) fall back to plain `border-radius`, and for the large surfaces that make the impression (window, sidebar inset, palette, dialogs, recorder capsule, toasts) draw the squircle with an SVG `clip-path` generated by `figma-squircle` (cornerSmoothing 0.6), with shadows on an unclipped parent.

- Radii: `radius-xs` 5 (kbd, checkbox), `radius-sm` 8 (rows, menu items), `radius-md` 10 (buttons, inputs), `radius-lg` 14 (menus, toasts, cards), `radius-xl` 20 (palette, dialogs, composer), `radius-window` 16, `radius-pill`.
- Nested shapes are concentric: inner radius = outer radius − inset (palette 20 − 12 padding = 8 for rows).
- Chips, the recorder capsule, the Dynamic Island, switches and avatars are pills or circles.

## Elevation

Depth comes from material first, shadow second. `shadow-xs` for controls that sit on a surface, `shadow-sm` for cards, `shadow-float` for anything floating (menus, toasts, capsule), `shadow-overlay` for the palette and dialogs. Dark-theme shadows add a 0.5px light rim because a shadow alone disappears on graphite. Glass adds an inner top highlight (`glass-highlight`) and a 0.5px edge (`glass-edge`); that rim is what makes it read as glass.

## Interaction states

Every interactive element has rest, hover, pressed, focus-visible and disabled states, and every change between them animates.

- **Hover**: a wash (`fill-hover`) or colour step fades in `--dur-fast`; icons inside play their one-shot motion; secondary buttons lift 1px; glass shows a pointer-following highlight.
- **Press**: scale to 0.97 (buttons) or 0.92 (icon buttons) in `--dur-instant`, release on `--spring-snappy`.
- **Selected**: `fill-press` plus `accent-text` icon; in lists the selection is one shared element that slides.
- **Disabled**: 45% opacity, no hover, no cursor change. Prefer hiding an action over showing it disabled with no explanation.
- **Loading**: the control keeps its size; its label crossfades to a shimmer label or a 14px spinner. Never shift layout.

## Iconography

Lucide icons (ISC licence), 1.75px stroke, 18px default and 16px in dense places, always `currentColor`. Icons have no boxes, tiles or backgrounds around them: an icon button is just the glyph until hover. The animated set comes from lucide-animated (MIT, built on Motion) where an icon exists there; otherwise the `nt-i-*` classes in bundle.css give a one-shot transform (plus rotates 90°, arrow nudges 3px, bell rings, trash tips, gear turns, undo winds back). Icons animate on the parent's hover or focus and on meaningful state change (the bell rings when a scheduled-task proposal arrives). They never loop, except the recording dot's breathing and the AI shimmer. The app icon is a cream tile with an amber margin rule, a dot in the margin and three ink text lines (sources in `apps/desktop/src-tauri/icons/source/`); the menu bar glyph in `shell/tray.rs` is the same mark as a template image. The wordmark is "Margin" in Inter at 640 weight.

## Accessibility

- Text contrast 4.5:1 (3:1 for 24px+), checked for both themes in tokens.json usage notes. Marks that carry meaning (speaker rings, `rec`, focus ring) meet 3:1.
- Reduce motion (OS setting): travel and springs become 120ms linear crossfades; the recording dot stops breathing; shimmer becomes static `ink-3` text. Feedback still happens, it just doesn't move.
- Reduce transparency (OS setting, or Settings › Appearance): every glass layer becomes its opaque ground (`bg-raised`) with the same edges and shadows.
- Every icon-only control has an `aria-label` and a tooltip. Everything is reachable by keyboard; see the shortcut registry in the product spec.
- Status never depends on colour alone.

## Implementation notes

- Styles: Tailwind v4 with these tokens as CSS variables (`tokens.css`), plus `components/bundle.css` for component classes and motion variables. Radix primitives for menus, dialogs, tooltips; dnd-kit for drag and drop; cmdk for the palette.
- Motion: the `motion` library for anything with layout, gestures or exit animations (`layout`, `layoutId`, `AnimatePresence`), using the spring presets in Motion. CSS transitions use the `--spring-*` `linear()` curves for simple state changes.
- Window: no native material; the shell paints opaque grounds (see `materials.md`). `window-vibrancy` stays wired behind `WINDOW_MATERIAL` in `shell/material.rs`.
