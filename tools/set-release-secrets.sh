#!/usr/bin/env bash
# Uploads the secrets the Release workflow needs to the GitHub repository, from the files they
# live in on this Mac. Nothing is printed or written to the repo.
#
#   tools/set-release-secrets.sh <owner/repo> <App Store Connect Key ID> <Issuer ID>
#
# Needs `gh` signed in with access to the repository, and these files:
#   ~/cilica-signing/devid.p12 (+ devid.p12.password)  Developer ID certificate and key
#   ~/.appstoreconnect/private_keys/AuthKey_<KeyID>.p8  App Store Connect API key
#   ~/.tauri/cilica-updater.key                         updater signing key
set -euo pipefail

# ───────── Arguments ─────────

if [[ $# -ne 3 ]]; then
  echo "Usage: $0 <owner/repo> <App Store Connect Key ID> <Issuer ID>" >&2
  exit 1
fi
REPO="$1"
KEY_ID="$2"
ISSUER_ID="$3"

SIGNING="$HOME/cilica-signing"
P8="$HOME/.appstoreconnect/private_keys/AuthKey_${KEY_ID}.p8"
UPDATER_KEY="$HOME/.tauri/cilica-updater.key"

for file in "$SIGNING/devid.p12" "$SIGNING/devid.p12.password" "$P8" "$UPDATER_KEY"; do
  [[ -f "$file" ]] || { echo "Missing $file. See docs/releasing.md." >&2; exit 1; }
done

# ───────── Upload ─────────

set_secret() { gh secret set "$1" --repo "$REPO" --body "$2" >/dev/null && echo "Set $1"; }

set_secret APPLE_CERTIFICATE "$(base64 -i "$SIGNING/devid.p12")"
set_secret APPLE_CERTIFICATE_PASSWORD "$(cat "$SIGNING/devid.p12.password")"
set_secret APPLE_SIGNING_IDENTITY "Developer ID Application: Neo Haggard (D97WL744BA)"
set_secret APPLE_API_KEY "$KEY_ID"
set_secret APPLE_API_ISSUER "$ISSUER_ID"
set_secret APPLE_API_KEY_P8 "$(cat "$P8")"
set_secret TAURI_SIGNING_PRIVATE_KEY "$(cat "$UPDATER_KEY")"
# The updater key has no password; GitHub rejects empty secrets, and the workflow treats a missing
# secret as empty, so this one is left unset.

echo "Done. Publish a release tagged like v0.2.0 to build it."
