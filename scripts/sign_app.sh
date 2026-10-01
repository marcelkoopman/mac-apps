#!/usr/bin/env bash
# Ad-hoc sign a cargo-bundle .app with its App Sandbox entitlements, the same way
# the release workflows do (release-copycraft.yml / release-ticker.yml).
#
# Usage: scripts/sign_app.sh <copycraft|ticker> [path/to/App.app]
#   Without a path (or APP_PATH) the newest of these is used:
#     target/aarch64-apple-darwin/release/bundle/osx/<Name>.app  (CI / --target build)
#     target/release/bundle/osx/<Name>.app                       (plain `cargo bundle --release`)
#
# Run it AFTER everything is copied into the bundle (including scripts/build_app_icon.sh):
# changing the bundle after signing breaks the signature.
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
die() { echo -e "${RED}$*${NC}" >&2; exit 1; }

PKG="${1:-}"
[ -n "$PKG" ] || die "Gebruik: $0 <copycraft|ticker> [pad/naar/App.app]"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOML="$ROOT/apps/$PKG/Cargo.toml"
ENTITLEMENTS="$ROOT/apps/$PKG/assets/entitlements.plist"
[ -f "$TOML" ] || die "Geen $TOML"
[ -f "$ENTITLEMENTS" ] || die "Geen $ENTITLEMENTS"
command -v codesign >/dev/null || die "codesign niet gevonden (alleen macOS)"

# Bundle name from [package.metadata.bundle] name = "..."
NAME="$(awk '
  /^\[package\.metadata\.bundle\][[:space:]]*$/ { s = 1; next }
  /^\[/ { s = 0 }
  s && /^name[[:space:]]*=/ { sub(/^name[[:space:]]*=[[:space:]]*"/, ""); sub(/".*$/, ""); print; exit }
' "$TOML")"
[ -n "$NAME" ] || die "Geen bundle-naam in [package.metadata.bundle] van $TOML"

APP="${2:-${APP_PATH:-}}"
if [ -z "$APP" ]; then
  for candidate in \
    "$ROOT/target/aarch64-apple-darwin/release/bundle/osx/$NAME.app" \
    "$ROOT/target/release/bundle/osx/$NAME.app"; do
    [ -d "$candidate" ] || continue
    if [ -z "$APP" ] || [ "$candidate" -nt "$APP" ]; then
      APP="$candidate"
    fi
  done
fi
[ -n "$APP" ] && [ -d "$APP" ] || die "Geen $NAME.app gevonden. Eerst: (cd apps/$PKG && cargo bundle --release [--target aarch64-apple-darwin])"

if [ "$PKG" = "ticker" ] && [ ! -f "$APP/Contents/Resources/assets/normal.png" ]; then
  echo -e "${YELLOW}⚠️  $APP/Contents/Resources/assets/normal.png ontbreekt. CI kopieert de tray-icons vóór het signen;" \
    "in de sandbox valt Ticker anders terug op een effen icoon (bronmap is niet leesbaar).${NC}" >&2
fi

plutil -lint "$ENTITLEMENTS" >/dev/null

echo -e "${YELLOW}🔏 Signing $APP${NC}"
# No --deep: the bundle holds one executable and no nested frameworks/helpers,
# and --deep would push these entitlements onto nested code as well.
codesign --force --sign - --options runtime --entitlements "$ENTITLEMENTS" "$APP"
codesign --verify --strict --verbose=2 "$APP"

echo "=== Signature ==="
codesign -dv "$APP" 2>&1 | grep -E '^(Identifier|CodeDirectory|Signature)' || true
echo "=== Entitlements ==="
codesign -d --entitlements - "$APP"

ENT_OUT="$(mktemp -t "$PKG-entitlements")"
trap 'rm -f "$ENT_OUT"' EXIT
codesign -d --entitlements - --xml "$APP" > "$ENT_OUT" 2>/dev/null
[ "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.app-sandbox' "$ENT_OUT" 2>/dev/null)" = "true" ] \
  || die "com.apple.security.app-sandbox staat niet op true in de signature"

echo -e "${GREEN}✅ $NAME.app ad-hoc gesigned met sandbox-entitlements${NC}"
