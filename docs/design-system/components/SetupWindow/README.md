# SetupWindow

First-run setup is its own small window (560 × 700, fixed size), not a dialog. One column over background art: a welcome screen, then steps that open one at a time and collapse to a line when done. Components live in `src/ui/Setup.tsx` and `src/ui/SceneArt.tsx`, styles in `src/ui/setup.css`; the flow is `src/features/onboarding/`.

**Art** (`SceneArt`): a dithered pixel ridgeline with drifting clouds, drawn on a canvas at a third of the window's size and scaled up without smoothing. Its colours are mixed from `bg-canvas`, `ink` and `scene-tint` (amber unless the chosen theme sets its own), so it is dawn in the light theme and night with stars in the dark one, and it repaints when the theme changes. It rises 40px and fades in over 1.6s once the window has painted. A 16px blur and a fade to `bg-canvas` cover the top 80%, so text never sits on the art; it reads clearly only at the bottom. Reduce motion keeps it still; Reduce transparency drops the blur layer.

**Title band** (`SetupBand`, 44px): macOS traffic lights (or Cilica's own close button on Windows and Linux), "Set up Cilica" centred, the step count ("2 of 5") and the background sound button. The title and count are hidden on the welcome screen.

**Welcome** (`SetupWelcome`): "Welcome to *Cilica*" in Newsreader 42px, a one-line lead, four features from the README (16px icon, a title, one line), and Set up Cilica (`nt-btn-lg`). It fades and lifts 12px away before the steps come in, so the window only ever has one heading.

**Steps** (`SetupStep`): the open step's title is Newsreader 30px with Skip (or Not now) beside it; its body opens through `grid-template-rows` on `--spring-smooth`. A finished step shrinks to one 12.5px line in `ink-3` with a `success` check (`ink-4` when skipped) that reopens it on click. Later steps are not shown. The open step scrolls into view after it has opened.

**Inside steps**:
- `SetupPermission`: a full-width button that invites ("Let Cilica hear you") and becomes a quiet confirmation ("Cilica can hear you"). The next one to grant takes the accent; the rest are glass. Optional ones carry a tag.
- `SetupPanel`: a glass block for related controls (the auto-detect switch and its mode).
- `SetupCard`: a raised card for a model or download, with a `Progress` bar while it downloads.
- `SetupChoice` in `SetupChoices`: radio cards (providers, typefaces); the chosen one gets a 1.5px `accent` ring. `font` sets the preview in that note typeface.
- `SetupApp` in `SetupApps`: an app found on this computer, its logo and count, with a check that springs in when picked.
- `SetupField` and `SetupFieldRow`: a label above every field, and a field with its button beside it (Save key, Connect).

**Background work** (`SetupTask`): pills along the bottom edge for downloads and imports that keep going while you move on, with a ring that fills (spinning while the size is unknown). They spring in on `--spring-bouncy` and leave when the work finishes; the finished toast says so.

**Glass** in the window follows Materials: `glass-fill-strong` with a 12px blur and the 0.5px `glass-edge`, opaque `bg-raised` under Reduce transparency. No inner highlight.
