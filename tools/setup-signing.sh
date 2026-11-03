#!/bin/bash
# One-time: creates a self-signed code-signing identity in its own keychain, so every build
# has the same signature and macOS keeps the Screen Recording permission across rebuilds.
#
# The keychain gets a random password, kept in your login keychain (service below): anyone who can
# sign with this identity inherits Ambilight's Screen Recording and System Audio permissions.
set -euo pipefail

NAME="Ambilight Local Signing"
KC="$HOME/Library/Keychains/ambilight-signing.keychain-db"
SERVICE="Ambilight signing keychain"
LEGACY_PW="ambilight-local" # what older versions of this script used

if [ -f "$KC" ]; then
    if security find-generic-password -a "$USER" -s "$SERVICE" -w >/dev/null 2>&1; then
        echo "Signing keychain already exists: $KC"
    else
        # Created by an older version with the fixed password: switch it to a random one.
        PW=$(openssl rand -hex 24)
        security set-keychain-password -o "$LEGACY_PW" -p "$PW" "$KC"
        security add-generic-password -a "$USER" -s "$SERVICE" -w "$PW" -U
        echo "Gave $KC a random password (stored in the login keychain as \"$SERVICE\")"
    fi
    exit 0
fi
PW=$(openssl rand -hex 24)

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/cert.cnf" <<EOF
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
EOF

openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$TMP/cert.cnf" \
    -keyout "$TMP/key.pem" -out "$TMP/cert.pem" 2>/dev/null
openssl pkcs12 -export -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -name "$NAME" \
    -out "$TMP/id.p12" -passout pass:"$PW" \
    -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 2>/dev/null \
    || openssl pkcs12 -export -legacy -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -name "$NAME" -out "$TMP/id.p12" -passout pass:"$PW"

security create-keychain -p "$PW" "$KC"
security set-keychain-settings "$KC"            # never auto-lock
security unlock-keychain -p "$PW" "$KC"
security import "$TMP/id.p12" -k "$KC" -P "$PW" -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$PW" "$KC" >/dev/null
security add-generic-password -a "$USER" -s "$SERVICE" -w "$PW" -U

echo "Created signing identity \"$NAME\" in $KC"
