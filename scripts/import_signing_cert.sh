#!/usr/bin/env bash
# Import a Developer ID Application certificate (.p12) into a temporary keychain so codesign can
# use it without a password prompt. For CI (release-<app>.yml); on your own Mac the certificate
# is in the login keychain and this script is not needed.
#
# Env (GitHub secrets in CI):
#   MACOS_CERT_P12        the .p12 file (certificate + private key), base64-encoded
#   MACOS_CERT_PASSWORD   password of the .p12
#   MACOS_SIGN_IDENTITY   e.g. "Developer ID Application: Name (TEAMID)", must be in the .p12
# Writes KEYCHAIN=<path> to $GITHUB_ENV when that is set (scripts/sign_app.sh reads it), and
# prints the path. Remove the keychain afterwards with: security delete-keychain "$KEYCHAIN"
set -euo pipefail

die() { echo "::error::$*" >&2; exit 1; }

: "${MACOS_CERT_P12:?MACOS_CERT_P12 ontbreekt}"
: "${MACOS_CERT_PASSWORD:?MACOS_CERT_PASSWORD ontbreekt}"
: "${MACOS_SIGN_IDENTITY:?MACOS_SIGN_IDENTITY ontbreekt}"
command -v security >/dev/null || die "security niet gevonden (alleen macOS)"

DIR="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/signing.XXXXXX")"
KEYCHAIN="$DIR/signing.keychain-db"
P12="$DIR/cert.p12"
trap 'rm -f "$P12" "$DIR/DeveloperIDG2CA.cer"' EXIT
KEYCHAIN_PASSWORD="$(openssl rand -base64 24)"

printf '%s' "$MACOS_CERT_P12" | tr -d '\r\n ' | base64 --decode > "$P12"
[ -s "$P12" ] || die "MACOS_CERT_P12 is leeg na base64-decode"

security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
# Lock after 6 hours, not on sleep: a release job never waits that long.
security set-keychain-settings -lut 21600 "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
security import "$P12" -k "$KEYCHAIN" -P "$MACOS_CERT_PASSWORD" -f pkcs12 \
  -T /usr/bin/codesign -T /usr/bin/security
# Let codesign use the private key without a UI prompt.
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PASSWORD" \
  "$KEYCHAIN" >/dev/null

# Search the temporary keychain first and keep the existing ones (bash 3.2: no mapfile).
KEYCHAINS=("$KEYCHAIN")
while IFS= read -r line; do
  line="$(echo "$line" | sed -e 's/^[[:space:]]*"//' -e 's/"[[:space:]]*$//')"
  [ -n "$line" ] && KEYCHAINS+=("$line")
done < <(security list-keychains -d user)
security list-keychains -d user -s "${KEYCHAINS[@]}"

has_identity() {
  security find-identity -v -p codesigning "$KEYCHAIN" | grep -Fq "\"$MACOS_SIGN_IDENTITY\""
}
if ! has_identity; then
  # -v only lists identities whose chain validates. Without the Developer ID intermediate in
  # the system keychain, add Apple's Developer ID G2 CA and look again.
  curl -fsSL -o "$DIR/DeveloperIDG2CA.cer" https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer
  security import "$DIR/DeveloperIDG2CA.cer" -k "$KEYCHAIN" >/dev/null || true
fi
if ! has_identity; then
  security find-identity -p codesigning "$KEYCHAIN" >&2 || true
  die "Identiteit \"$MACOS_SIGN_IDENTITY\" niet (geldig) gevonden in de .p12"
fi

echo "Developer ID-identiteit staat in $KEYCHAIN"
if [ -n "${GITHUB_ENV:-}" ]; then
  echo "KEYCHAIN=$KEYCHAIN" >> "$GITHUB_ENV"
fi
echo "$KEYCHAIN"
