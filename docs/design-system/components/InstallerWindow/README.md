# InstallerWindow

Cilica Setup's window on Windows: install, update or remove Cilica in three screens (an update Cilica downloaded itself skips the first). Frameless, 600 × 480, fixed size; Windows 11 rounds its corners and draws its shadow. Components live in `src/ui/Installer.tsx`, styles in `src/ui/installer.css`; the flow is `src/features/installer/`. It is a sibling of the SetupWindow and shares its art, type and glass.

**Art** (`SceneArt`): the setup ridgeline, drawn 125% of the window's height and anchored to the bottom so the ridges sit low. A fade to `bg-canvas` covers the top (solid to 30%, 70% at 46%, clear at 72%). The `sun` prop tells the job's story: it rises from just over the ridges to clear of them as Cilica installs, and sets the same way as Cilica is removed, easing toward each new height. Dawn in the light theme, night with stars in the dark one; Setup has no Cilica settings to read, so it follows the Windows theme.

**Title band** (`InstallerBand`, 44px): Cilica's tile at 16px, the window's name ("Cilica Setup", "Uninstall Cilica") in `ink-3`, then Minimise and Close (`WindowControls`). The band drags the window. Close is disabled while Setup is working: stopping the installer halfway would leave Cilica broken.

**Screens** (`InstallerScreen`): a Newsreader 34px heading with *Cilica* in italic, a 14px lead in `ink-2`, then the screen's parts. Each new screen lifts 12px in on `--spring-smooth`. One primary button per screen, `nt-btn-lg`, at the start of `InstallerActions`; Close and Keep Cilica are ghost buttons beside it.
- **Install Cilica**: Install, and beside it "For you only, no administrator needed." with an Options link. Options opens an `InstallerOptions` glass panel through `grid-template-rows` on `--spring-smooth`: Folder (mono path, Change opens the Windows folder picker), Desktop shortcut and Open Cilica when you sign in, both off. The size is a footnote at the bottom. When Cilica is installed the screen is **Update** (or **Reinstall**) with the folder shown and no options.
- **Installing Cilica**: `InstallerProgress`: what is happening now with its percentage in mono, the 4px `Progress` bar, then a line in `ink-3` with a `success` check for each finished step, lifting in as it lands. The taskbar button carries the same bar. On failure the heading says what didn't happen ("Cilica wasn't installed"), `InstallerProblem` says why and what to do, the bar turns `danger`, and Try again leads.
- **Cilica is installed**: one line on what comes next, Open Cilica (it opens on Cilica's own setup window), Close.
- **Updating Cilica** (an update Cilica handed over at Restart): no first screen. The window opens on progress with an `InstallerHint` naming the two versions and saying notes and settings stay; the first step waits for Cilica to close. The sun rises as for an install. **Cilica is updated** then says it is opening Cilica, with Open Cilica already loading, and Setup closes once Cilica starts (1.2 s after the screen lands). If Cilica doesn't open, the problem shows with Open Cilica and Close. A failure leads with Try again, which starts the update again.
- **Remove Cilica?**: one `InstallerOption` in a glass panel: Also delete notes, recordings and settings, off by default. Off, its line names the folder the notes stay in; on, the switch turns `danger`, the line says it can't be undone, and the button becomes the `danger` "Remove Cilica and notes". Keep Cilica takes the focus, so Enter never deletes anything.
- **Cilica is removed**: what was kept or deleted, and Close.

**Glass** follows Materials: `glass-fill-strong`, a 12px blur and the 0.5px `glass-edge` with `shadow-sm`; opaque `bg-raised` in the browser preview. No inner highlight.

**Reduce motion**: the art stays still and the sun jumps to its height; screens, lines and the options panel appear without moving.

Preview: `bun run dev:web`, then `?window=installer` (`&flow=uninstall`, `&flow=update` for an update Cilica handed over, `&existing` for the Update screen, `&fail=copy` for a failure).
