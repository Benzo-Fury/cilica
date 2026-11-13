# Releasing Cilica

macOS builds are signed with a **Developer ID** certificate and notarized by Apple, then distributed directly (no App Store review). Updates are delivered with Tauri's updater from GitHub Releases.

## Targets

| Platform | Architecture | Signing |
| --- | --- | --- |
| macOS 14.4+ | Apple Silicon only (`aarch64-apple-darwin`) | Developer ID + notarization |
| Windows | x86_64 | Optional (Authenticode; avoids SmartScreen warnings). Ships Cilica Setup only |
| Linux | x86_64 | None |

No Intel or universal Mac build: `ort` ships no prebuilt ONNX Runtime for `x86_64-apple-darwin`, so Intel would mean building ONNX Runtime from source on every release. Intel Macs on 14.4+ are a small share, and on-device Whisper runs poorly on them anyway.

## One-time setup

1. **Developer ID Application certificate.** Xcode → Settings → Accounts → select the team → Manage Certificates → + → *Developer ID Application*. (Only the team's Account Holder can create one.) Check it with:
   `security find-identity -v -p codesigning` (it should list `Developer ID Application: … (TEAMID)`).
2. **App Store Connect API key** for notarization. App Store Connect → Users and Access → Integrations → Team Keys → generate a key with the *Developer* role. Download the `.p8` (you only get one chance) to `~/.appstoreconnect/private_keys/`. Note the Key ID and Issuer ID.
3. **Updater key.** Already generated at `~/.tauri/cilica-updater.key` (no password; public key is in `tauri.conf.json`). Back it up somewhere safe, such as a password manager. If you lose it, installed copies can never update again.
4. **Updater endpoint.** Set to `Benzo-Fury/cilica` in `plugins.updater.endpoints` in `apps/desktop/src-tauri/tauri.conf.json`.

## Local signed build

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Neo Haggard (D97WL744BA)"
export APPLE_API_KEY="<Key ID>"
export APPLE_API_ISSUER="<Issuer ID>"
export APPLE_API_KEY_PATH="$HOME/.appstoreconnect/private_keys/AuthKey_<Key ID>.p8"
export TAURI_SIGNING_PRIVATE_KEY="$HOME/.tauri/cilica-updater.key"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
bun run --cwd apps/desktop tauri build --target aarch64-apple-darwin
```

Output lands in `target/aarch64-apple-darwin/release/bundle/`: the notarized `.dmg`, plus `Cilica.app.tar.gz` and its `.sig` for the updater.

The `.dmg` opens on a "Drag Cilica into Applications" window: the setup window's night ridgeline from `apps/desktop/src-tauri/dmg/background.tiff`, with the icons placed by `bundle.macOS.dmg` in `tauri.conf.json`. The image is generated (`bun run dmg-background`), so change the script in `tools/` rather than the file. The window is laid out by Finder over AppleScript, so the first local build asks for your terminal to control Finder; allow it.

Verify:

```bash
spctl -a -vvv -t install target/aarch64-apple-darwin/release/bundle/macos/Cilica.app
```

## Windows installer

Windows ships one file, **`Cilica_<version>_x64_Setup.exe`**, Cilica Setup (`apps/setup`, docs/architecture.md › Windows installer). It is what people download and also the updater's package, signed for the updater as `Cilica_<version>_x64_Setup.exe.sig`. Tauri's NSIS installer is only built to go inside it, and the MSI isn't built: neither goes on the release, so there is no second installer to pick by mistake.

Setup is built on Windows around Tauri's build, by `tools/build-windows-setup.ts`:

```bash
bun run windows-setup
```

1. `prepare`: builds the frontend and a copy of Setup that can only remove Cilica, into `target/windows-setup/cilica-setup.exe`.
2. `tauri build --config src-tauri/tauri.windows-setup.conf.json`: Cilica's NSIS installer (the only Windows bundle that config makes), with that copy bundled next to Cilica.
3. `package`: Setup with the NSIS installer inside, into `target/release/bundle/setup/`. With `TAURI_SIGNING_PRIVATE_KEY` set it also writes the updater signature next to it.

CI runs `prepare` before tauri-action, which only builds on Windows, and `package` after it, then attaches Setup and its signature to the release. Once every platform is done, the `windows-updater` job points latest.json's `windows-x86_64` and `windows-x86_64-nsis` entries at Setup. To sign both copies of Setup, set `WINDOWS_SIGN_COMMAND` to a signing command with `%1` for the file; unsigned, SmartScreen warns the first time it runs. The NSIS installer inside Setup is signed through Tauri's Windows signing settings, which are not set up yet.

Try the window without Windows: `bun run setup` (`-- --uninstall` for removal, `-- --update x.exe --version 9.9.9` for an update) runs it against a simulated backend; start `bun run dev:web` first. On Windows, try the real thing in a VM: install, update over it with Setup, update from inside Cilica (Settings › General, then Restart), remove from Settings › Apps with and without deleting notes, and a PC without WebView2.

## Releasing from GitHub

[`.github/workflows/release.yml`](../.github/workflows/release.yml) runs when a GitHub release is **published**. It builds macOS (Apple Silicon), Windows and Linux on separate runners, signs and notarizes the Mac build, and attaches every installer to the release, plus the signed updater packages and `latest.json`. The version lives in one place, `[workspace.package]` in the root `Cargo.toml`; the crates, the Tauri bundle and the updater all read it. To release, bump it, commit and push, then publish a release whose tag matches (`v0.2.1` for 0.2.1). The workflow fails fast if the tag and the version disagree. A failing platform doesn't stop the others from shipping.

To release: create a release on GitHub with a new tag such as `v0.2.0`, write the notes (they appear in the update prompt), and publish it. Watch the Actions tab; the installers appear on the release when each job finishes.

### One-time: repository secrets

Once the repo exists and the App Store Connect key is downloaded:

```bash
tools/set-release-secrets.sh <owner/repo> <Key ID> <Issuer ID>
```

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | `~/cilica-signing/devid.p12`, base64 |
| `APPLE_CERTIFICATE_PASSWORD` | `~/cilica-signing/devid.p12.password` |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Neo Haggard (D97WL744BA)` |
| `APPLE_API_KEY`, `APPLE_API_ISSUER` | Key ID and Issuer ID |
| `APPLE_API_KEY_P8` | Contents of the `.p8` file (the workflow writes it to a temporary file) |
| `TAURI_SIGNING_PRIVATE_KEY` | Contents of `~/.tauri/cilica-updater.key` |

Only the macOS job uses the Apple secrets.

## Test builds for pull requests

[`.github/workflows/test-builds.yml`](../.github/workflows/test-builds.yml) builds Cilica for macOS (Apple Silicon, `.dmg`), Windows (Cilica Setup) and Linux (AppImage and `.deb`) on every pull request that touches the app, its crates or the build. The installers appear under **Artifacts** on the workflow run and are kept for 14 days; nothing is published. A new push to the pull request cancels the build still running for the old one. Docs-only pull requests don't build. To build any branch without a pull request, run **Test builds** from the Actions tab.

Test builds use no secrets: they are unsigned and have no updater packages, so an installed test build never offers or takes an update. macOS asks you to open the app from Finder's context menu (Open) the first time; Windows SmartScreen warns until you choose More info › Run anyway.

## Updates

Installed copies check `latest.json` on the latest GitHub release 30 seconds after launch and every six hours. With **Settings › General › Install updates automatically** on (the default), a new version downloads and installs in the background, then a toast offers to restart. On Windows it only downloads in the background; Restart closes Cilica and starts the Cilica Setup it downloaded, which installs the update in its window and opens Cilica again. Copies from before 0.6 start it through Tauri's updater, and it shows the same window; Cilica 0.6 runs it silently under its own copy of Setup. With it off, the toast offers to install. The same page has **Check for updates**. Checks are skipped in `bun run dev`. In the browser preview, add `?update` to the URL to see the update flow.

The updater only accepts packages signed with `~/.tauri/cilica-updater.key`. On Linux, automatic updates work for the AppImage only; `.deb` installs update through the package manager.
