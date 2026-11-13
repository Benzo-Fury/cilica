# Releasing Cilica

macOS builds are signed with a **Developer ID** certificate and notarized by Apple, then distributed directly (no App Store review). Updates are delivered with Tauri's updater from GitHub Releases.

## Targets

| Platform | Architecture | Signing |
| --- | --- | --- |
| macOS 14.4+ | Apple Silicon only (`aarch64-apple-darwin`) | Developer ID + notarization |
| Windows | x86_64 | Optional (Authenticode; avoids SmartScreen warnings) |
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

## Updates

Installed copies check `latest.json` on the latest GitHub release 30 seconds after launch and every six hours. With **Settings › General › Install updates automatically** on (the default), a new version downloads and installs in the background, then a toast offers to restart. With it off, the toast offers to install. The same page has **Check for updates**. Checks are skipped in `bun run dev`. In the browser preview, add `?update` to the URL to see the update flow.

The updater only accepts packages signed with `~/.tauri/cilica-updater.key`. On Linux, automatic updates work for the AppImage only; `.deb` installs update through the package manager.
