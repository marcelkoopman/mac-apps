#!/usr/bin/env bash
# Check the Apple privacy manifest (and the export-compliance key) of an app. macOS only: it
# needs plutil and PlistBuddy. The Linux side is the Rust test apps/<app>/tests/privacy_manifest.rs.
#
# Usage: scripts/check_privacy_manifest.sh <copycraft|ticker> [path/to/App.app]
#   Without an .app: lints the source manifest apps/<app>/PrivacyInfo.xcprivacy.
#   With an .app: also checks that Contents/Resources/PrivacyInfo.xcprivacy exists, lints, and
#   equals the source, and that Contents/Info.plist carries ITSAppUsesNonExemptEncryption=false
#   when the app's assets/Info.plist.ext sets it (copycraft). Run it before signing (changing
#   the bundle afterwards breaks the signature) and again after, to see the signed bundle.
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; NC='\033[0m'
die() { echo -e "${RED}$*${NC}" >&2; exit 1; }

PKG="${1:-}"
[ -n "$PKG" ] || die "Usage: $0 <copycraft|ticker> [path/to/App.app]"
APP="${2:-}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/apps/$PKG/PrivacyInfo.xcprivacy"
EXT="$ROOT/apps/$PKG/assets/Info.plist.ext"
[ -f "$SRC" ] || die "No $SRC"
command -v plutil >/dev/null || die "plutil not found (macOS only)"

check_manifest() {
  plutil -lint "$1" >/dev/null || die "plutil -lint failed for $1"
  [ "$(/usr/libexec/PlistBuddy -c 'Print :NSPrivacyTracking' "$1")" = "false" ] \
    || die "NSPrivacyTracking is not false in $1"
  /usr/libexec/PlistBuddy -c 'Print :NSPrivacyAccessedAPITypes:0:NSPrivacyAccessedAPIType' "$1" >/dev/null \
    || die "No NSPrivacyAccessedAPITypes entries in $1"
}

check_manifest "$SRC"
echo -e "${GREEN}$PKG: $SRC parses${NC}"

if [ -n "$APP" ]; then
  [ -d "$APP" ] || die "No app bundle at $APP"
  BUNDLED="$APP/Contents/Resources/PrivacyInfo.xcprivacy"
  [ -f "$BUNDLED" ] || die "PrivacyInfo.xcprivacy is not in the bundle: $BUNDLED"
  check_manifest "$BUNDLED"
  cmp -s "$SRC" "$BUNDLED" || die "$BUNDLED differs from $SRC"
  if grep -q ITSAppUsesNonExemptEncryption "$EXT"; then
    [ "$(/usr/libexec/PlistBuddy -c 'Print :ITSAppUsesNonExemptEncryption' "$APP/Contents/Info.plist")" = "false" ] \
      || die "ITSAppUsesNonExemptEncryption is not false in $APP/Contents/Info.plist"
  fi
  echo -e "${GREEN}$PKG: bundle holds a valid, identical PrivacyInfo.xcprivacy${NC}"
fi
