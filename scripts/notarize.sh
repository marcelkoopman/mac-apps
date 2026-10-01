#!/usr/bin/env bash
# Notarize a Developer ID-signed .app or .dmg with `xcrun notarytool submit --wait`, staple the
# ticket and check it with Gatekeeper (spctl). Used by release-<app>.yml when the secrets are
# there; also works on your own Mac.
#
# Usage: scripts/notarize.sh <path/to/App.app | path/to/file.dmg>
#
# Credentials, the first complete set wins:
#   App Store Connect API key (recommended):
#     APPLE_API_KEY_P8     contents of AuthKey_<id>.p8 (the PEM text, or that text base64-encoded)
#     APPLE_API_KEY_ID     key ID (10 characters)
#     APPLE_API_ISSUER_ID  issuer ID (UUID) from App Store Connect > Users and Access > Integrations
#   Apple ID:
#     APPLE_ID             Apple ID e-mail address
#     APPLE_TEAM_ID        team ID (10 characters)
#     APPLE_APP_PASSWORD   app-specific password from account.apple.com
set -euo pipefail

die() { echo "::error::$*" >&2; exit 1; }

TARGET="${1:-}"
[ -n "$TARGET" ] && [ -e "$TARGET" ] || die "Gebruik: $0 <App.app|file.dmg>"
command -v xcrun >/dev/null || die "xcrun niet gevonden (alleen macOS)"

WORK="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/notarize.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

AUTH=()
if [ -n "${APPLE_API_KEY_P8:-}" ] && [ -n "${APPLE_API_KEY_ID:-}" ] && [ -n "${APPLE_API_ISSUER_ID:-}" ]; then
  KEY="$WORK/AuthKey_${APPLE_API_KEY_ID}.p8"
  if printf '%s' "$APPLE_API_KEY_P8" | grep -q 'BEGIN PRIVATE KEY'; then
    printf '%s\n' "$APPLE_API_KEY_P8" > "$KEY"
  else
    printf '%s' "$APPLE_API_KEY_P8" | tr -d '\r\n ' | base64 --decode > "$KEY"
  fi
  AUTH=(--key "$KEY" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID")
  echo "Notarisatie met App Store Connect API-key $APPLE_API_KEY_ID"
elif [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ] && [ -n "${APPLE_APP_PASSWORD:-}" ]; then
  AUTH=(--apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD")
  echo "Notarisatie met Apple ID (team $APPLE_TEAM_ID)"
else
  die "Geen notarytool-credentials: zet APPLE_API_KEY_P8/APPLE_API_KEY_ID/APPLE_API_ISSUER_ID of APPLE_ID/APPLE_TEAM_ID/APPLE_APP_PASSWORD"
fi

case "$TARGET" in
  *.app | *.app/)
    TARGET="${TARGET%/}"
    # notarytool takes a zip, dmg or pkg, not a bundle directory.
    SUBMIT="$WORK/$(basename "$TARGET" .app).zip"
    ditto -c -k --keepParent "$TARGET" "$SUBMIT"
    KIND=app
    ;;
  *.dmg)
    SUBMIT="$TARGET"
    KIND=dmg
    ;;
  *) die "Alleen .app of .dmg: $TARGET" ;;
esac

echo "=== notarytool submit $(basename "$SUBMIT") (wacht op Apple) ==="
RESULT="$WORK/submit.json"
set +e
xcrun notarytool submit "$SUBMIT" "${AUTH[@]}" --wait --timeout 30m --output-format json > "$RESULT"
SUBMIT_EXIT=$?
set -e
cat "$RESULT"; echo
ID="$(plutil -extract id raw -o - "$RESULT" 2>/dev/null || true)"
STATUS="$(plutil -extract status raw -o - "$RESULT" 2>/dev/null || true)"
if [ "$STATUS" != "Accepted" ]; then
  if [ -n "$ID" ]; then
    echo "=== notarytool log $ID ===" >&2
    xcrun notarytool log "$ID" "${AUTH[@]}" >&2 || true
  fi
  die "Notarisatie niet geaccepteerd (status: ${STATUS:-onbekend}, exit $SUBMIT_EXIT)"
fi

echo "=== stapler ==="
xcrun stapler staple "$TARGET"
xcrun stapler validate "$TARGET"

echo "=== Gatekeeper ==="
if [ "$KIND" = app ]; then
  spctl --assess --type execute --verbose=2 "$TARGET"
else
  spctl --assess --type open --context context:primary-signature --verbose=2 "$TARGET"
fi
echo "✅ $(basename "$TARGET") genotariseerd en gestapled"
