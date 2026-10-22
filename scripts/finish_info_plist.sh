#!/usr/bin/env bash
# Usage: scripts/finish_info_plist.sh <pkg> <path/to/App.app>
# macOS only (PlistBuddy). Run after cargo bundle and before signing: it changes the bundle.
# cargo-bundle always writes CFBundleDevelopmentRegion=English. Copycraft is localised (en, nl,
# CFBundleLocalizations in assets/Info.plist.ext), so its development region is the ISO code
# `en`. Then checks the Info.plist keys App Store review looks at.
set -euo pipefail
PKG="${1:?pkg}"
APP="${2:?app path}"
PLIST="$APP/Contents/Info.plist"
PB=/usr/libexec/PlistBuddy
die() { echo "finish_info_plist: $*" >&2; exit 1; }
[ -f "$PLIST" ] || die "no $PLIST"
[ "$PKG" = copycraft ] || exit 0

"$PB" -c 'Set :CFBundleDevelopmentRegion en' "$PLIST" 2>/dev/null \
  || "$PB" -c 'Add :CFBundleDevelopmentRegion string en' "$PLIST"

expect() {
  local got
  got="$("$PB" -c "Print :$1" "$PLIST" 2>/dev/null)" || die "$1 missing"
  [ "$got" = "$2" ] || die "$1 is '$got', expected '$2'"
}
expect CFBundleDevelopmentRegion en
expect 'CFBundleLocalizations:0' en
expect 'CFBundleLocalizations:1' nl
expect NSHumanReadableCopyright '© 2026 Marcel Koopman'
expect LSApplicationCategoryType public.app-category.utilities
expect CFBundleIdentifier nl.marcelkoopman.copycraft
plutil -lint "$PLIST" >/dev/null || die "plutil -lint failed"
echo "finish_info_plist: $PLIST ok"
