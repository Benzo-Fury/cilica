Cilica mobile is the phone companion to the desktop app: read and write notes, ask Cilica about them, and record what happens in the room. It ships as two native apps, SwiftUI on iOS and Jetpack Compose on Android, on top of the shared Rust crates.

This folder is the mobile design system. It inherits Cilica's identity from the desktop system in `docs/design-system/`: the warm graphite palette, the one accent, the note typography, the voice and the springy motion. What changes on a phone is the chrome. **Each platform gets its own native chrome (Liquid Glass on iOS, Material 3 Expressive on Android) and Cilica owns everything inside it.** People should feel at home in the shell and recognise Cilica in the content, the way the Claude app does.

| File | What it covers |
| --- | --- |
| `README.md` | Shared principles, navigation model, colour, type, layout, motion, haptics, accessibility |
| `ios.md` | Everything iOS-only: Liquid Glass, navigation and sheets, Dynamic Type, SF Symbols, Live Activity, widgets, SwiftUI mapping |
| `android.md` | Everything Android-only: Material 3 Expressive roles, edge-to-edge, predictive back, foreground-service recording, Glance widgets, Compose mapping |
| `tokens.json` | Mobile tokens: shared colour (same values as desktop), per-platform type, spacing, sizes, radii, springs, haptics |
| `references.md` | What was borrowed from which app, with Mobbin links |
| `components/<Name>/README.md` + `preview.html` | Each component, with the iOS and Android versions side by side |
| `components/bundle.css` | Reference CSS for the previews only. The apps use SwiftUI and Compose, not this file |
| `screens.html` | The key screens on both platforms, light and dark |

## Principles

1. **Native shell, Cilica content.** Navigation bars, sheets, menus, switches, keyboards and system surfaces are the platform's own components, so they behave correctly with every gesture and every accessibility setting. Notes, transcripts, the assistant thread and the recorder are Cilica's own, drawn with the shared tokens.
2. **Content first, one thing floating.** A screen is one opaque page (`bg-canvas`). At most one floating cluster sits over it at a time: the composer, the recorder bar or the format bar. Never two at once.
3. **Monochrome first, one accent.** Graphite `accent` marks the single most important control on a screen. `rec` red means recording and nothing else. Cilica does not use Material You wallpaper colour or the iOS tint for brand; see each platform file for the exceptions.
4. **Ask, write, record: one thumb away.** The three verbs of the app sit in the bottom third of the screen. The composer is at the bottom, the record button is in the composer, and "New note" is in the bottom bar or a FAB.
5. **Everything that moves, springs.** Same four spring presets as desktop, translated to SwiftUI and Compose parameters (see Motion). Navigation transitions stay native.
6. **Quiet, with small delights.** The record button morphs into send, a tick draws itself, notes ready lands with a soft haptic. No confetti, no mascots.

## Voice and copy

Identical to desktop (`docs/design-system/README.md`, Voice and copy). On mobile:

- Buttons are short verbs: "Record", "New note", "Ask", "Stop", "Delete note". Never "OK".
- Privacy copy names the device: "Audio stays on this iPhone", "Audio stays on this phone" (Android). Never "on the device".
- Recording copy is honest about what a phone can hear: "Recording what this phone hears. Put it near the people talking." Mobile cannot capture other apps' call audio, so never promise "Records your calls".
- The assistant is "Cilica". The composer placeholder is "Ask Cilica" (home) or "Ask about this note" (a note).

## Navigation model

A **left drawer** (Claude-style) over a single main stack. There is no tab bar.

```
Drawer (swipe from the left edge, or tap ☰)
├─ Search field
├─ Home          ← default destination
├─ Notes
├─ Meetings
├─ Folders       (section, user folders)
├─ Recents       (section, last 8 notes and threads)
└─ Footer: avatar (opens Settings) · "New note" pill
```

| Screen | What it shows | Bottom of screen |
| --- | --- | --- |
| **Home** | A short greeting, "Up next" (the next calendar event, when connected), 3 recent notes, recent Cilica threads | Composer: "Ask Cilica", with the record button |
| **Notes / Meetings** | Notes grouped by date (Today, Yesterday, Previous 7 days, then month names) | iOS: search pill + compose button. Android: search bar in the top app bar + "New note" extended FAB |
| **Note** | Title, meta chips (date, people, folder), body. Meetings add a Notes / Transcript switch | Composer pill: "Ask about this note". While editing, the format bar replaces it above the keyboard |
| **Recording** | A meeting note that is recording: live transcript or your notes | Recorder bar: level, timer, Pause, Stop |
| **Thread** | A conversation with Cilica (full screen from Home; a sheet from a note) | Composer |
| **Settings** | Grouped rows. A sheet on iOS, a full screen on Android | None |

Rules:
- The drawer is for places, not actions. The only action in it is "New note".
- From inside a note, Cilica opens as a **sheet** over the note (so the note stays visible above it), never as a push.
- Back always returns along the stack. The drawer is never reached by Back.
- On tablets and foldables (≥ 840dp / regular width) the drawer becomes a permanent sidebar and the note list and note sit side by side, like desktop.

## Colour

Mobile uses **the same colour tokens and values as desktop** (`docs/design-system/tokens.json`), so a note looks identical on both. `tokens.json` here lists them again with their platform role mappings. Looks (Ridgeline, Tide, …) are desktop-only in v1; mobile has Light, Dark and System.

How the grounds are used on a phone:

| Token | Mobile use |
| --- | --- |
| `bg-canvas` | Every main screen: home, lists, notes, threads |
| `bg-sunken` | The drawer, and the ground behind grouped lists (Settings) |
| `bg-raised` | Grouped list cells, cards (Up next, diff cards), dialogs, sheets in dark mode |
| `fill-press` | User message bubbles, chips, secondary buttons, the pressed state of rows |
| `accent` / `on-accent` | One primary control per screen: the composer's send/record button, "New note" |
| `rec` / `rec-soft` | Recording only: the record dot, the recorder bar timer, the Live Activity |

Dark mode uses the warm graphite grounds, never pure black, except for the Dynamic Island and AOD/lock-screen surfaces, which are the system's.

## Typography

**Interface text is the system font**, so it scales with Dynamic Type and Android font scale and matches the platform: SF Pro on iOS, the system sans (Roboto on Pixel, the OEM font elsewhere) on Android. **Note content keeps Cilica's fonts**: Inter for the note title and body, Newsreader for the Serif note font, Geist Mono for timecodes and code. All bundled fonts scale with the OS text size.

Each platform's full scale is in its file. The shared roles:

| Role | iOS | Android | Notes |
| --- | --- | --- | --- |
| Screen title (large) | Large Title, 34 | Headline medium, 28 | Notes, Meetings, Home greeting |
| Bar title (inline) | Headline, 17 semibold | Title large, 22 | |
| Row title | Body, 17 | Body large, 16 | Note titles in lists |
| Row meta / snippet | Subheadline, 15, `ink-2` | Body medium, 14, `ink-2` | |
| Section header | Footnote, 13 semibold, `ink-3`, sentence case | Title small, 14, `ink-3` | "Today", "Recents". No uppercase |
| Button | Body, 17 semibold | Label large, 14 medium | |
| Note title | Inter 28 / 34, 640 | Inter 28 / 34, 640 | Scales with text size |
| Note body | Inter 17 / 26, 420 | Inter 16 / 25, 420 | Each platform's reading size |
| Note H1 / H2 / H3 | Inter 22 / 19 / 17 | Inter 22 / 19 / 16 | |
| Transcript | Inter 16 / 24 | Inter 16 / 24 | |
| Timecode | Geist Mono 13, tabular | Geist Mono 12, tabular | |

Every live number (timer, counts) uses tabular figures.

## Layout and density

- **Grid**: 4pt/4dp. Screen side margin 16 on phones, 24 on tablets. The note column is capped at the desktop measure (720) on tablets.
- **Touch targets**: 44 × 44pt on iOS, 48 × 48dp on Android, even when the glyph is smaller.
- **Rows**: a note row is two lines (title + meta/snippet). Its minimum height is 60pt on iOS and 72dp on Android (M3 two-line list item). Single-line rows are 44pt / 56dp.
- **Bottom clearance**: content scrolls under the floating composer or recorder bar, with a bottom inset equal to its height plus 16, so the last line can always be read above it.
- **Safe areas**: content goes edge to edge behind the status bar and home indicator / gesture bar. Only controls respect the safe area.

## Shape

- Both platforms use continuous (squircle) corners for Cilica's own surfaces: `RoundedRectangle(style: .continuous)` on iOS and `RoundedCornerShape` (with `AbsoluteSmoothCornerShape` from a squircle library when available) on Android.
- Buttons, chips, the composer's send button, the recorder bar and the search field are pills or circles on both platforms.
- The composer and cards follow each platform's corner family: 26 on iOS (concentric with the device corners), 28 on Android (M3 Extra Large).
- Nested shapes are concentric: inner radius = outer radius − inset.

## Materials

- **Main screens are opaque** `bg-canvas`, as on desktop. Reading text never sits on glass.
- **iOS** uses system Liquid Glass for the navigation bar's buttons, toolbars, the bottom search bar, the composer and the recorder bar (see `ios.md`). **Android** uses tonal surfaces (`bg-raised`, M3 surface containers) with M3 elevation, no blur (see `android.md`).
- Under Reduce Transparency (iOS) or when blur is unavailable (Android below 12), glass becomes `bg-raised` with the same edge and shadow.

## Motion

The desktop presets, in each toolkit. Use them for Cilica's own components. System transitions (push, sheets, drawer, predictive back) stay native and untouched.

| Preset | SwiftUI | Compose | Use for |
| --- | --- | --- | --- |
| snappy | `.spring(duration: 0.2, bounce: 0.15)` | `spring(dampingRatio = 0.85f, stiffness = 987f)` | Press release, chips, checkboxes, the format bar appearing |
| smooth | `.spring(duration: 0.3, bounce: 0.1)` | `spring(dampingRatio = 0.9f, stiffness = 439f)` | Segmented thumb, list insert/remove, diff card collapse, the composer growing |
| bouncy | `.spring(duration: 0.35, bounce: 0.3)` | `spring(dampingRatio = 0.7f, stiffness = 322f)` | Record ⇄ send morph, recorder bar expanding, toast arriving, a dropped note landing |
| gentle | `.spring(duration: 0.45, bounce: 0)` | `spring(dampingRatio = 1f, stiffness = 195f)` | Notes ⇄ Transcript, summary streaming in, empty states |

Compose values come from SwiftUI's definition: stiffness = (2π / duration)², damping ratio = 1 − bounce.

The desktop rules still apply: enter on a spring, exit on a quick fade (120ms); one moving element for selection; lists reorder instead of jumping; animate transform and opacity only; gestures follow the finger, then hand their velocity to a spring.

**Signature moments on mobile**
- **Start recording**: the composer's record button (a dot) grows into the recorder bar (`bouncy`), the timer starts, the start haptic fires and the start tone plays. On iOS the Live Activity appears in the Dynamic Island. On Android the ongoing notification and status-bar chip appear.
- **Stop**: Stop morphs into a check, the bar shows "Writing notes…" in shimmer, the summary streams into the note (`gentle`, word by word), then the success haptic plays and the bar shrinks back into the composer.
- **Record ⇄ send**: on Home, an empty composer's round button is Record (a dot). As soon as you type, it morphs into Send (an arrow) with `bouncy`. Clearing the text morphs it back. Everywhere else (a note, a thread) the button is always Send, dimmed while the composer is empty.

## Sound and haptics

Sounds are the desktop ones: the start and stop tones and the notes-ready chime. They respect the ring/silent switch (iOS) and the media volume (Android). Start and stop always play, because they are about consent.

Haptics fire only on meaningful moments, never on scroll or typing, and respect the system's haptics setting and Cilica's own "Haptic feedback" switch in Settings.

| Moment | iOS (`sensoryFeedback`) | Android (`HapticFeedbackConstants`) |
| --- | --- | --- |
| Recording started | `.start` | `TOGGLE_ON` (API 34+), else `CONFIRM` |
| Recording stopped | `.stop` | `TOGGLE_OFF` (API 34+), else `CONFIRM` |
| Notes ready / edit accepted | `.success` | `CONFIRM` |
| Edit rejected, action failed | `.error` | `REJECT` |
| Long-press opens a menu, drag starts | system (menu) / `.impact(weight: .medium)` | `LONG_PRESS` / `DRAG_START` |
| A dragged note snaps into a folder | `.alignment` | `SEGMENT_TICK` |
| Segmented control, picker detents | `.selection` | `SEGMENT_FREQUENT_TICK` |
| Switch toggled | system | `TOGGLE_ON` / `TOGGLE_OFF` |

## Accessibility

- Text contrast is 4.5:1 for all `ink`, `ink-2` and `ink-3` text on every ground in both themes (same as desktop).
- Everything scales to the largest accessibility text sizes (AX5 on iOS, 200% on Android). At accessibility sizes, rows drop the snippet and wrap the title; the composer grows upward; icon-only bars keep 44/48 targets.
- Every icon-only control has a label (`accessibilityLabel` / `contentDescription`). The record button reads "Record", then "Stop recording, 12 minutes".
- Recording state is announced ("Recording started") and never shown by colour alone: the dot breathes and the timer runs.
- Reduce Motion (iOS) and Remove animations (Android): springs become 120ms crossfades, the record dot stops breathing, shimmer becomes static `ink-3` text.
- Respect Bold Text, Increase Contrast (use `line-strong` for every hairline) and Button Shapes on iOS.

## Implementation notes

- **Shared core**: notes, search, the agent, transcription and storage come from the Rust crates through a UniFFI layer. UI state lives in each app (an `@Observable` model on iOS, a `ViewModel` + `StateFlow` on Android).
- **Tokens**: generate `CilicaColors.swift` / `Color.kt`, type and spacing constants from `tokens.json` (a generator script is a follow-up). Never hard-code a hex in a view.
- **Icons**: SF Symbols on iOS and Material Symbols Rounded on Android, so icons match the system font weight. The mapping table is in each platform file. Cilica-only glyphs (the Ridgeline mark, the record dot) are custom assets.
- **Minimum OS**: iOS 26 (Liquid Glass APIs), Android 10 (API 29), with Android 12+ for blur and splash, and 16+ for Live Updates. Older paths are listed in each platform file.
