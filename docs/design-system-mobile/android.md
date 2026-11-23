# Android

Everything here applies to Android phones, tablets and foldables only. Shared rules are in `README.md`. Build with **Jetpack Compose and Material 3 Expressive** (`androidx.compose.material3`), min SDK 29, target the latest SDK.

## Material 3, Cilica-coloured

Cilica uses Material 3 components and layout, with Cilica's colour scheme, its note typography and its motion on top. People should recognise the Android patterns (top app bar, FAB, navigation drawer, bottom sheets, snackbars) and recognise Cilica in the content.

### Colour scheme

**Dynamic colour (Material You) is off.** Cilica's identity is one graphite accent on warm neutrals, the same on every device. The `ColorScheme` is built from the shared tokens:

| M3 role | Light / dark from token | Notes |
| --- | --- | --- |
| `primary` / `onPrimary` | `accent` / `on-accent` | FAB, filled buttons, switch on, send |
| `primaryContainer` / `onPrimaryContainer` | `accent-soft` / `accent-text` | Selected drawer item, selected chips |
| `secondaryContainer` / `onSecondaryContainer` | `fill-press` on `bg-canvas`, flattened / `ink` | Tonal buttons, user message bubbles |
| `surface`, `background` | `bg-canvas` | Every main screen |
| `surfaceContainerLowest` | `bg-raised` (light), `bg-sunken` (dark) | |
| `surfaceContainerLow` | `bg-sunken` | Navigation drawer, the ground behind settings groups |
| `surfaceContainer` | `bg-sunken` (light), `bg-raised` (dark) | Search bar, composer |
| `surfaceContainerHigh` | `bg-raised` | Cards, bottom sheets, menus, dialogs |
| `surfaceContainerHighest` | `bg-window` | Pressed cards, input fields on cards |
| `onSurface` / `onSurfaceVariant` | `ink` / `ink-2` | `ink-3` is a Cilica extension colour |
| `outline` / `outlineVariant` | `line-strong` / `line` (flattened to opaque) | |
| `error` / `errorContainer` | `danger` / `danger-soft` | |
| `scrim` | `scrim` | |
| `tertiary` | not used | |

Cilica-only colours (`ink-3`, `rec`, `rec-soft`, `success`, `warning`, `info`, the `speaker-*` set) live in a `CilicaColors` object exposed through a `CompositionLocal`, next to `MaterialTheme.colorScheme`.

Translucent tokens (`line`, `fill-press`) are flattened onto `bg-canvas` for M3 roles, because M3 roles are expected to be opaque.

### Typography

M3 `Typography` uses the system sans (`FontFamily.Default`), so it follows the device font and font scale. Cilica sets the styles it uses and leaves the rest at M3 defaults.

| Cilica role | M3 style | Colour |
| --- | --- | --- |
| Large screen title (LargeTopAppBar, Home greeting) | `headlineMedium` 28 | `onSurface` |
| Top app bar title | `titleLarge` 22 | `onSurface` |
| Card and sheet titles | `titleMedium` 16 / 500 | `onSurface` |
| Section header | `titleSmall` 14 / 500 | `ink-3`, sentence case |
| Row title | `bodyLarge` 16 | `onSurface` |
| Row meta, descriptions | `bodyMedium` 14 | `onSurfaceVariant` |
| Buttons, chips, tabs | `labelLarge` 14 / 500 | depends |
| Badges, tool steps | `labelMedium` 12 / 500 | `onSurfaceVariant` |

Note text uses bundled Inter, Newsreader and Geist Mono in `sp`, so it honours the font scale up to 200% (non-linear font scaling on Android 14+). Inter's `cv05` and `cv08` are on through `fontFeatureSettings = "cv05, cv08"`.

### Shape

The M3 Expressive shape scale, with Cilica's choices:

| Use | Shape |
| --- | --- |
| Buttons, chips, search bar, recorder bar | `CircleShape` / full |
| Extended FAB "New note" | M3 default (large, 16) |
| Cards, diff cards | `medium` 12 |
| Menus, snackbars | `large` 16 (snackbar is pill-shaped in Expressive) |
| Composer, dialogs, bottom sheet top corners | `extraLarge` 28 |
| Navigation drawer trailing corners | `large` 16 |

Use a smooth-corner shape where available so the squircle character carries over from iOS and desktop. M3 Expressive shape morphing (`MaterialShapes`) is allowed in one place: the record button morphs from a circle (Record) to a rounded square (Stop).

### Motion

- Set `MaterialTheme(motionScheme = MotionScheme.standard())`: Cilica is quick and calm, so the expressive bounce is reserved for Cilica's own signature moments.
- Cilica components use the four springs from `README.md` (`spring(dampingRatio, stiffness)`).
- Screen transitions are Navigation Compose's defaults, with **predictive back** on (`android:enableOnBackInvokedCallback="true"`). Sheets, the drawer and the search view all animate along with the back gesture.
- Press feedback is M3's ripple, recoloured to `ink` at 10% on Cilica surfaces. Cilica's own floating controls (record/send, recorder bar) also scale to 0.94 on press, released on `snappy`.

## Edge to edge and system bars

- `enableEdgeToEdge()` on every activity. Content draws behind the status and navigation bars; controls use `WindowInsets.safeDrawing`.
- Status bar icons follow the theme (dark icons on light `bg-canvas`).
- Gesture navigation: the drawer's edge swipe must not fight the system back gesture. The drawer opens only from ☰ and from a drag inside the root screen's content. On deeper screens the left edge is back.
- The composer and recorder bar sit above `WindowInsets.ime` and `navigationBars`, using `imePadding()`. With 3-button navigation, the bar sits above the buttons.

## Navigation

| Pattern | M3 component | Spec |
| --- | --- | --- |
| Drawer | `ModalNavigationDrawer` + `ModalDrawerSheet` | `surfaceContainerLow` ground; search field at the top; `NavigationDrawerItem`s for Home, Notes, Meetings; "Folders" and "Recents" sections with `titleSmall` headers; footer with the avatar (Settings) and a filled "New note" button. The selected item uses `primaryContainer` |
| Top app bar | `TopAppBar` (most screens), `LargeTopAppBar` (Notes, Meetings, Settings) | ☰ or back as the navigation icon; at most two action icons plus overflow. Collapses on scroll (`exitUntilCollapsedScrollBehavior`) |
| Search | `SearchBar` / `ExpandedFullScreenSearchBar` (Expressive) in the top app bar on list screens | Placeholder "Search notes". Expands to a full-screen search view with recent searches |
| New note | `ExtendedFloatingActionButton` "New note" on Notes and Meetings | Collapses to an icon FAB on scroll. Not shown on Home (the composer is there) or on a note |
| Record (Home) | Inside the composer, not a FAB | See Composer |
| Note screen | `TopAppBar` with back, the Notes / Transcript switch (meetings), share and overflow | The toolbar while editing is a docked `HorizontalFloatingToolbar` above the IME |
| Cilica from a note | `ModalBottomSheet`, partially expanded by default, draggable to full height | `skipPartiallyExpanded = false` |
| Settings | A full-screen destination (not a sheet) with `LargeTopAppBar` | Grouped `ListItem`s on `surfaceContainerHigh` cards over `surfaceContainerLow`, as in Android 16 Settings |
| Note actions | Long-press a row opens a `DropdownMenu` anchored to it; the note's overflow (⋮) does the same | Destructive item last, in `error` |
| Swipe on rows | `SwipeToDismissBox` | End-to-start: Delete (with an Undo snackbar). Start-to-end: Pin |
| Confirmations | Snackbar with Undo, not dialogs | Dialogs (`AlertDialog`) only for irreversible actions with no undo |

## Iconography: Material Symbols

Material Symbols **Rounded**, weight 400, fill 0 (fill 1 for the selected drawer item), optical size 24. Rounded matches Cilica's soft squircle shapes.

| Meaning | Material Symbol |
| --- | --- |
| Drawer | `menu` |
| New note | `edit_square` |
| Search | `search` |
| Record | custom Cilica record glyph (filled circle in a ring) |
| Stop | `stop` (filled) |
| Pause | `pause` (filled) |
| Send | `arrow_upward` |
| Attach | `add` |
| Notes | `description` |
| Meetings | `graphic_eq` |
| Folder | `folder` |
| Share | `share` |
| More | `more_vert` |
| Delete | `delete` |
| Pin | `keep` |
| Transcript | `subtitles` |
| Settings | `settings` |
| Undo | `undo` |

The More icon is `more_vert` (vertical) on Android and `ellipsis` (horizontal) on iOS. Share is `share` here, never the iOS square-and-arrow.

## Recording on Android

- **What it records**: the microphone. Android does not let apps capture call audio, and VoIP apps opt out of playback capture, so copy says "Recording what this phone hears".
- **Foreground service** with `foregroundServiceType="microphone"` (Android 14 requires it). Start it only from a visible user action (the record button, the Quick Settings tile, the widget), never from the background.
- **Ongoing notification** (channel "Recording", importance low, no sound):
  - Small icon: Cilica's mark. Title: the note title. Text: "Recording · 12:04" with a `Chronometer` timer.
  - Actions: **Pause** and **Stop**. Content tap opens the recording note.
  - Colour: `setColor(rec)` for the small icon accent. Never colorize the whole notification; the system styles it.
  - **Android 16+ Live Update**: request promotion (`setRequestPromotedOngoing(true)`) so recording shows as a status-bar chip with the timer and on the lock screen, like an iOS Live Activity.
  - While writing notes: "Writing notes…" with an indeterminate progress bar. Then a separate "Notes ready" notification (channel "Notes ready", default importance), which opens the note.
- **Privacy indicator**: Android shows its green mic chip. Cilica's own red dot never imitates it.
- **Audio focus and calls**: when a phone call starts, pause and post "Paused by a call" in the notification. Resume needs a tap.
- **Permissions**: ask for `RECORD_AUDIO` the first time someone taps Record, with a short sheet beforehand: "Cilica needs the microphone to record. Audio stays on this phone." Ask for `POST_NOTIFICATIONS` (Android 13+) right after, explaining that it shows the recording controls.

## System integration

| Surface | Spec |
| --- | --- |
| Home screen widgets (Glance) | "Record" (1×1, one tap starts recording through a foreground-service-allowed `PendingIntent`), "Recent notes" (resizable list + Record). `GlanceTheme` with Cilica colours; support the system's dynamic widget colours only for the widget background, never for the record glyph |
| Quick Settings tile | "Record with Cilica" (`TileService`), active state while recording, with the timer as the subtitle |
| Share target | "Save to Cilica" in the share sheet (`ACTION_SEND` text and URLs): a `ModalBottomSheet` with a title, a folder picker and Save |
| App shortcuts | Long-press the launcher icon: New note, Record, Ask Cilica (static shortcuts) |
| Themed icon | A monochrome layer of the Ridgeline mark for Android 13+ themed icons |
| Splash | `SplashScreen` API with the Ridgeline icon on `bg-canvas`, no animation |
| Predictive back to home | The app's root handles back by closing the drawer first, then the system's back-to-home animation |
| Hardware keyboards and ChromeOS | Ctrl+N, Ctrl+F, Ctrl+K, Ctrl+J and Ctrl+E mirror desktop. The keyboard shortcuts helper (Meta+/) lists them |

## Editing

- The note editor is a `BasicTextField` with a rich-text state (Markdown round-trips in the shared core).
- The **format bar** is a `HorizontalFloatingToolbar` (M3 Expressive) docked above the IME: text style, checklist, bulleted list, link, attach, and "Ask Cilica" on the selection. It scrolls horizontally when it overflows.
- **Text style** opens a small `ModalBottomSheet` with paragraph styles (Title, Heading, Subheading, Body, Mono) as a single-select button group, then B / I / U / S toggles.
- The selection toolbar gains "Ask Cilica" and "Rewrite" through `ACTION_PROCESS_TEXT` handling inside the app.

## Haptics

Use `LocalHapticFeedback` / `View.performHapticFeedback` with `HapticFeedbackConstants`, following the mapping in `README.md`. M3 components (switches, sliders, long-press) already give their own feedback; don't double up.

## Tablets and foldables

- Use `NavigationSuiteScaffold` / window size classes. At Expanded width (≥ 840dp) the drawer becomes a `PermanentNavigationDrawer`, and the list and note use `ListDetailPaneScaffold`. Cilica threads open in a supporting pane (`SupportingPaneScaffold`), like the desktop assistant.
- Foldables in tabletop posture during recording: the transcript goes on the top half and the recorder controls on the bottom half.

## Fallbacks

| Feature | Fallback |
| --- | --- |
| Live Updates (Android 16) | Standard ongoing notification |
| `TOGGLE_ON` / `TOGGLE_OFF` haptics (API 34) | `CONFIRM` |
| Blur (API 31) | Not used anyway; tonal surfaces only |
| Predictive back animations (API 34) | Standard back |
| Themed icons (API 33) | Standard adaptive icon |
