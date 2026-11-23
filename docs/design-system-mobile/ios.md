# iOS

Everything here applies to iPhone and iPad only. Shared rules are in `README.md`. Target **iOS 26+** with SwiftUI; drop to UIKit only where SwiftUI has no API (the keyboard accessory bar, the text editor's rich text).

## Liquid Glass

iOS 26 draws navigation chrome in Liquid Glass. Cilica uses the system glass and never imitates it.

| Surface | Material | API |
| --- | --- | --- |
| Navigation bar buttons (☰, back, share, more) | System glass, grouped automatically | `.toolbar { ToolbarItem … }` |
| Bottom toolbar on lists (search + compose) | System glass | `.searchable` with `.searchToolbarBehavior(.minimize)` plus a `ToolbarItem(placement: .bottomBar)` |
| Composer | Glass, regular | `.glassEffect(.regular, in: .rect(cornerRadius: 26))` |
| Recorder bar | Glass, regular, tinted with `rec` only on the dot | `.glassEffect(.regular, in: .capsule)` |
| Format bar above the keyboard | Glass, regular | `.glassEffect` inside a `ToolbarItemGroup(placement: .keyboard)` |
| Record / send button | Glass, prominent, `accent` tint | `.buttonStyle(.glassProminent).tint(.cilica.accent)` |
| Sheets | System (glass at partial height, opaque at full) | `.sheet` + `.presentationDetents` |
| Menus | System | `Menu`, `.contextMenu` |

Rules:
- **Glass is for controls; content stays on `bg-canvas`.** Never put note text, transcript lines or replies on glass.
- **Group neighbours** with `GlassEffectContainer` so the composer's record button and the recorder bar can morph into each other (`.glassEffectID(_:in:)`).
- **Tint sparingly.** Only the one prominent button per screen carries the graphite `accent` tint. Never tint the whole bar.
- **Reduce Transparency**: the system handles its own glass; for Cilica's `.glassEffect` surfaces, swap to `bg-raised` with a 0.5pt `glass-edge` stroke when `accessibilityReduceTransparency` is on.

## Navigation

- One `NavigationStack` for the main stack. The **drawer is custom**: the stack slides right by the drawer width, with a `scrim` over it. It opens from ☰ and from an edge pan starting within 20pt of the left edge. That pan must not break back-swipe: inside a pushed screen, the edge pan is the system back gesture, and the drawer opens only from the root screen.
- The drawer content sits on `bg-sunken`, under the status bar, with no glass.
- **Large titles** on Notes, Meetings and Settings (`.navigationBarTitleDisplayMode(.large)`); **inline** on a note (the note's own title is the large text, so the bar shows it only after it scrolls away).
- **Search** lives in the bottom toolbar on list screens and minimises into a glass button while scrolling. In the drawer, search is a plain field at the top.
- **Sheets**:
  - Cilica from a note: `.presentationDetents([.medium, .large])`, `.presentationBackgroundInteraction(.enabled(upThrough: .medium))` so you can keep scrolling the note behind it.
  - Settings: a full-height sheet with a ✕ close button (top left) and a NavigationStack inside.
  - Note actions and pickers (Move to folder, Note font): `.medium` detent.
- **Context menus** on note rows: long-press opens `.contextMenu` with a preview of the note. Destructive items use `role: .destructive` and come last.
- **Swipe actions** on note rows: trailing swipe gives Delete (destructive, full swipe allowed) and Move. Leading swipe gives Pin.

## Typography

SF Pro through Dynamic Type text styles. Never set a fixed point size on interface text.

| Cilica role | Text style | Weight | Colour |
| --- | --- | --- | --- |
| Screen title | `.largeTitle` | bold (system) | `ink` |
| Home greeting | `.largeTitle`, Newsreader under "Serif note font" | regular | `ink` |
| Bar title | `.headline` | semibold | `ink` |
| Row title | `.body` | regular, `.medium` for unread meeting summaries | `ink` |
| Row meta | `.subheadline` | regular | `ink-2` |
| Section header | `.footnote` | semibold | `ink-3` |
| Chip | `.footnote` | medium | `ink-2` |
| Button | `.body` | semibold | depends |
| Footer under a group | `.footnote` | regular | `ink-3` |

**Note fonts** (Inter, Newsreader, Geist Mono) are bundled and scaled with `UIFontMetrics(forTextStyle:)` (`.custom("Inter", size: 17, relativeTo: .body)` in SwiftUI), so they follow Dynamic Type up to AX5. Turn on Inter's `cv05` and `cv08` features, as on desktop.

## Colour mapping

Use Cilica tokens, not semantic UIKit colours, for content and grounds, so both platforms and desktop match. Set the app's global tint to `accent` so system controls (switch on, selection) are graphite.

| Token | iOS use |
| --- | --- |
| `bg-canvas` | `.background(Color.cilica.canvas)` on every main screen. Replaces `systemBackground` |
| `bg-sunken` | Drawer ground; `.scrollContentBackground(.hidden)` + this ground behind grouped lists |
| `bg-raised` | `.listRowBackground` of grouped cells, cards |
| `ink` / `ink-2` / `ink-3` / `ink-4` | `label` / `secondaryLabel` / `tertiaryLabel` / `placeholderText` roles |
| `line` | `.listRowSeparatorTint` |
| `accent` | `.tint`, the prominent button |
| `rec` | The recording dot, the recorder timer, the Live Activity |
| `danger` | `role: .destructive` gets the system red; Cilica's own destructive text uses `danger` |

Switches are tinted with `accent` (graphite), not the system green, matching desktop. Increase Contrast swaps `line` for `line-strong`.

## Shapes

- Continuous corners everywhere: `RoundedRectangle(cornerRadius: r, style: .continuous)`, `.rect(cornerRadius:)`.
- Grouped lists use `.listStyle(.insetGrouped)`, whose iOS 26 section radius (≈26) matches `radius-xl`.
- Use `ConcentricRectangle` / `.containerShape` for anything inset in a sheet or the composer, so its corners stay concentric with the container and the device.

## Iconography: SF Symbols

Use SF Symbols at the text style of their label, so they scale and match weight. Default rendering is monochrome `ink-2`; selected is `ink` (not tinted). The table maps Cilica's desktop Lucide names to symbols.

| Meaning | Lucide (desktop) | SF Symbol |
| --- | --- | --- |
| Drawer | `menu` | `line.3.horizontal` |
| New note | `square-pen` | `square.and.pencil` |
| Search | `search` | `magnifyingglass` |
| Record | (custom dot) | custom `cilica.record` (filled circle in a ring) |
| Stop | `square` | `stop.fill` |
| Pause | `pause` | `pause.fill` |
| Send | `arrow-up` | `arrow.up` |
| Attach | `plus` | `plus` |
| Notes | `file-text` | `doc.text` |
| Meetings | `audio-lines` | `waveform` |
| Folder | `folder` | `folder` |
| Share | `share` | `square.and.arrow.up` |
| More | `ellipsis` | `ellipsis` |
| Delete | `trash-2` | `trash` |
| Pin | `pin` | `pin` |
| Transcript | `captions` | `captions.bubble` |
| Cilica | Ridgeline mark | custom `cilica.mark` |
| Settings | `settings` | `gearshape` |
| Undo | `undo-2` | `arrow.uturn.backward` |

Symbol effects stand in for desktop's one-shot icon motion: `.symbolEffect(.bounce)` on tap for New note and Pin, `.symbolEffect(.wiggle)` for the bell, `.symbolEffect(.breathe)` on the recording dot (stopped under Reduce Motion), and `.contentTransition(.symbolEffect(.replace))` for Record ⇄ Stop ⇄ check.

## Recording on iOS

- **What it records**: the microphone only. iOS does not let an app capture other apps' call audio. Copy says "Recording what this iPhone hears".
- **Background**: the `audio` background mode keeps capture running when the app is backgrounded or the screen is locked. Transcription continues on device.
- **Live Activity** (ActivityKit) starts with every recording:
  - **Compact** (Dynamic Island): the leading side shows the `rec` dot (breathing); the trailing side shows the timer (`Text(timerInterval:)`, monospaced digits).
  - **Minimal**: the `rec` dot only.
  - **Expanded**: note title, timer, a live level waveform, and Pause and Stop buttons through App Intents (`LiveActivityIntent`).
  - **Lock Screen**: the same as expanded on a `bg-canvas`-dark ground (the system tints it), with "Audio stays on this iPhone" as the footer.
  - While Cilica writes the notes, it shows "Writing notes…". It ends with "Notes ready" and a deep link to the note, then dismisses after 4 hours or when opened.
- **Interruptions**: on a phone call or Siri, recording pauses automatically and the Live Activity shows "Paused by a call". Resume needs a tap; never resume silently.
- **Privacy indicator**: iOS shows its orange mic dot. Cilica's `rec` red never sits next to it in the status bar.
- **Control Center and Action button**: a `ControlWidget` "Record with Cilica" toggle, and an App Intent so the Action button can start a recording.

## System integration

| Surface | Spec |
| --- | --- |
| Home Screen widgets (WidgetKit) | Small: "Record" (one tap starts a recording via an App Intent). Medium: the 3 latest notes + Record. All on `bg-canvas`, with tinted and clear rendering modes supported (`widgetAccentable` on the mark only) |
| Lock Screen widget | Accessory circular: the record glyph |
| Share extension | "Save to Cilica" from Safari, Mail or any text: creates a note with the shared content and source link. A compact sheet with a title field, a folder picker and Save |
| App Intents / Shortcuts | Start recording, New note, Ask Cilica (takes text, returns the reply), Append to note |
| Spotlight | Index note titles and bodies with Core Spotlight; tapping opens the note |
| Handoff | A note open on iPhone continues on the Mac app (`NSUserActivity` with the note id) |
| Keyboard shortcuts (iPad) | Mirror desktop's ⌘N, ⌘F, ⌘K (palette as a sheet), ⌘J (focus composer), ⌘E (Notes ⇄ Transcript) |

## The keyboard and editing

- The note editor is a `TextEditor` with `AttributedString` (iOS 26 rich text) and Markdown round-tripping in the shared core.
- The **format bar** is a glass capsule docked above the keyboard (`ToolbarItemGroup(placement: .keyboard)`): Aa (opens the format sheet), checklist, list, link, attach, and Ask Cilica on the selection. It scrolls horizontally when it overflows; it never wraps.
- **Aa** opens a format panel in place of the keyboard, as Apple Notes does: a row of paragraph styles (Title, Heading, Subheading, Body, Mono), then B / I / U / S, then list and indent controls.
- **Done** is a glass checkmark in the navigation bar while editing. It dismisses the keyboard; it is not a save (notes save continuously).
- Selecting text adds "Ask Cilica" and "Rewrite with Cilica" to the edit menu (`UIEditMenuInteraction` / SwiftUI `.textSelectionAffordances`).

## Haptics

Use SwiftUI's `.sensoryFeedback(_:trigger:)`, with the mapping in `README.md`. System controls (switches, pickers, context menus, pull to refresh) already play their own; don't add a second one.

## iPad

- Regular width: `NavigationSplitView` with the drawer as the sidebar (`bg-sunken`), the note list as the content column and the note as the detail. The composer docks to the bottom of the detail column; Cilica threads open in an inspector (`.inspector`), like the desktop assistant panel.
- Stage Manager and Split View: the layout follows size class, never the device type.
- Pointer: rows and buttons get `.hoverEffect(.highlight)`; text in notes keeps the I-beam.

## Minimum OS fallbacks

The design targets iOS 26. If iOS 18 support is ever required: replace `.glassEffect` with `.ultraThinMaterial` plus a 0.5pt `glass-edge` stroke, and replace `.glassProminent` with a solid `accent` capsule. Everything else works unchanged.
