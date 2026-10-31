#!/bin/bash
# Re-signs a local build with the self-signed identity from tools/setup-signing.sh, so macOS
# keeps the Screen Recording permission across rebuilds. Without it, builds stay ad-hoc signed.
# Usage: tools/sign-macos.sh [path/to/Ambilight.app]
set -euo pipefail

APP="${1:-target/release/bundle/macos/Ambilight.app}"
KC="$HOME/Library/Keychains/ambilight-signing.keychain-db"

if [ ! -f "$KC" ]; then
    echo "No local signing identity; run tools/setup-signing.sh first (keeping ad-hoc signature)."
    exit 0
fi

# codesign only finds identities in the keychain search list: add ours briefly, then restore.
ORIG_KEYCHAINS=$(security list-keychains -d user | tr -d '"' | xargs)
trap 'security list-keychains -d user -s $ORIG_KEYCHAINS' EXIT
security list-keychains -d user -s $ORIG_KEYCHAINS "$KC"
security unlock-keychain -p ambilight-local "$KC"
codesign --force --deep --sign "Ambilight Local Signing" "$APP"
codesign -dr - "$APP" 2>&1 | tail -1
