#!/bin/bash
# Creates a self-signed code-signing identity in its own keychain, so every build has the same
# signature and macOS keeps the Screen Recording permission across rebuilds.
#
# Usage: tools/setup-signing.sh [--rotate] [--export DIR]
#   --rotate      replace an existing identity with a new one (macOS then asks for permissions once)
#   --export DIR  also write DIR/signing.p12 and DIR/signing.p12-password, e.g. for the release
#                 workflow's MACOS_SIGNING_P12 secrets; delete them after uploading
#
# Anyone who can sign with this identity inherits Ambilight's Screen Recording and System Audio
# permissions, so the keychain gets a random password, kept in your login keychain, and is locked
# except while tools/sign-macos.sh signs.
set -euo pipefail

NAME="Ambilight Local Signing"
KC="$HOME/Library/Keychains/ambilight-codesign.keychain-db"
SERVICE="Ambilight codesign keychain"
# Older versions of this script used this keychain, protected by a fixed password published in
# this repository; its key is replaced rather than reused.
LEGACY_KC="$HOME/Library/Keychains/ambilight-signing.keychain-db"

ROTATE=false
EXPORT_DIR=""
while [ $# -gt 0 ]; do
    case "$1" in
        --rotate) ROTATE=true ;;
        --export) EXPORT_DIR="${2:?--export needs a directory}"; shift ;;
        *) echo "Unknown option: $1"; exit 2 ;;
    esac
    shift
done

if [ -f "$LEGACY_KC" ]; then
    echo "Removing the signing identity made by an older version of this script; creating a new one."
    security delete-keychain "$LEGACY_KC" 2>/dev/null || rm -f "$LEGACY_KC"
    security delete-generic-password -a "$USER" -s "Ambilight signing keychain" >/dev/null 2>&1 || true
fi

if [ -f "$KC" ]; then
    if ! security find-generic-password -a "$USER" -s "$SERVICE" >/dev/null 2>&1; then
        ROTATE=true # the password is missing, so the keychain can't be unlocked
    fi
    if ! $ROTATE; then
        echo "Signing keychain already exists: $KC (use --rotate to replace it)"
        exit 0
    fi
    security delete-keychain "$KC"
    security delete-generic-password -a "$USER" -s "$SERVICE" >/dev/null 2>&1 || true
fi

PW=$(openssl rand -hex 24)
P12_PW=$(openssl rand -hex 24)
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/cert.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CNF

openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$TMP/cert.cnf" \
    -keyout "$TMP/key.pem" -out "$TMP/cert.pem" 2>/dev/null
openssl pkcs12 -export -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -name "$NAME" \
    -out "$TMP/id.p12" -passout pass:"$P12_PW" \
    -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 2>/dev/null \
    || openssl pkcs12 -export -legacy -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -name "$NAME" -out "$TMP/id.p12" -passout pass:"$P12_PW"

security create-keychain -p "$PW" "$KC"
security set-keychain-settings "$KC"            # no timeout; tools/sign-macos.sh locks it after use
security unlock-keychain -p "$PW" "$KC"
security import "$TMP/id.p12" -k "$KC" -P "$P12_PW" -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$PW" "$KC" >/dev/null
security lock-keychain "$KC"
security add-generic-password -a "$USER" -s "$SERVICE" -w "$PW" -U
# Check that the stored password really unlocks the keychain before relying on it.
security unlock-keychain -p "$(security find-generic-password -a "$USER" -s "$SERVICE" -w)" "$KC"
security lock-keychain "$KC"

if [ -n "$EXPORT_DIR" ]; then
    mkdir -p "$EXPORT_DIR"
    (umask 077; cp "$TMP/id.p12" "$EXPORT_DIR/signing.p12"; printf %s "$P12_PW" > "$EXPORT_DIR/signing.p12-password")
    echo "Wrote $EXPORT_DIR/signing.p12 and signing.p12-password; delete them after uploading."
fi
echo "Created signing identity \"$NAME\" in $KC"
openssl x509 -in "$TMP/cert.pem" -noout -fingerprint -sha1
