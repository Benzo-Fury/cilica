#!/usr/bin/env bash
# Cargo runner for macOS dev builds (see .cargo/config.toml).
#
# Cargo leaves debug binaries ad-hoc signed, so their code signature changes on every rebuild and
# macOS forgets "Always Allow" on the keychain item that holds the API key. Signing the app binary
# with a stable Apple Development identity keeps the signature the same across rebuilds, so the
# keychain prompt is answered once.
#
# Identity: $CILICA_DEV_SIGNING_IDENTITY if set, otherwise the first "Apple Development" identity in
# the login keychain. With neither, the binary runs unsigned as before. Test and other binaries are
# never touched.
set -euo pipefail

bin="$1"
shift

# Sign only the app binary
if [[ "$(basename "$bin")" == "cilica-desktop" ]]; then
  # Pick the signing identity
  identity="${CILICA_DEV_SIGNING_IDENTITY:-}"
  if [[ -z "$identity" ]]; then
    identity="$(security find-identity -v -p codesigning 2>/dev/null \
      | awk -F'"' '/Apple Development/ { print $2; exit }')"
  fi

  # Sign with a fixed identifier so the keychain sees the same app every build
  if [[ -n "$identity" ]]; then
    codesign --force --sign "$identity" --identifier app.cilica.desktop.dev "$bin" \
      || echo "dev-sign-run: signing failed, running unsigned (expect keychain prompts)" >&2
  else
    echo "dev-sign-run: no Apple Development identity found, running unsigned (expect keychain prompts)" >&2
  fi
fi

exec "$bin" "$@"
