#!/bin/bash
# Re-signs a local build with the self-signed identity from tools/setup-signing.sh, so macOS
# keeps the Screen Recording permission across rebuilds. Without it, builds stay ad-hoc signed.
# Usage: tools/sign-macos.sh [path/to/Ambilight.app]
set -euo pipefail

APP="${1:-target/release/bundle/macos/Ambilight.app}"
KC="$HOME/Library/Keychains/ambilight-codesign.keychain-db"

if [ ! -f "$KC" ] && [ ! -f "$HOME/Library/Keychains/ambilight-signing.keychain-db" ]; then
    echo "No local signing identity; run tools/setup-signing.sh first (keeping ad-hoc signature)."
    exit 0
fi
if [ -f "$HOME/Library/Keychains/ambilight-signing.keychain-db" ]; then
    echo "Found a signing identity made by an older tools/setup-signing.sh; run it again to replace it."
    exit 1
fi
if ! PW=$(security find-generic-password -a "$USER" -s "Ambilight codesign keychain" -w 2>/dev/null); then
    echo "The signing keychain's password is missing; run tools/setup-signing.sh --rotate."
    exit 1
fi

# codesign only finds identities in the keychain search list: add ours briefly, then restore the
# list and lock the keychain again.
ORIG_KEYCHAINS=()
while IFS= read -r line; do
    line="${line#"${line%%[![:space:]]*}"}"
    ORIG_KEYCHAINS+=("${line//\"/}")
done < <(security list-keychains -d user)
restore() {
    security list-keychains -d user -s "${ORIG_KEYCHAINS[@]}"
    security lock-keychain "$KC"
}
trap restore EXIT
security list-keychains -d user -s "${ORIG_KEYCHAINS[@]}" "$KC"
security unlock-keychain -p "$PW" "$KC"
# Hardened runtime, as in releases: stops other code being injected into the app to borrow its
# Screen Recording and System Audio permissions.
codesign --force --deep --options runtime --sign "Ambilight Local Signing" "$APP"
codesign -dr - "$APP" 2>&1 | tail -1
