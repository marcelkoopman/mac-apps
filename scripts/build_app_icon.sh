#!/usr/bin/env bash
# Compile the app icon into a cargo-bundle .app with actool, the same way the release workflows
# do (release-copycraft.yml / release-ticker.yml). cargo-bundle only copies an .icns, and it
# cannot handle the macOS 26 Icon Composer format (.icon). This adds:
#   Contents/Resources/Assets.car      app icon from an asset catalog (CFBundleIconName); on
#                                      macOS 26 the Liquid Glass icon when the source is a .icon
#   Contents/Resources/AppIcon.icns    the same icon for older macOS and Finder (CFBundleIconFile)
#
# Usage: scripts/build_app_icon.sh <copycraft|ticker> [path/to/App.app]
#   Without a path (or APP_PATH) the newest of these is used, as in scripts/sign_app.sh:
#     target/aarch64-apple-darwin/release/bundle/osx/<Name>.app  (CI / --target build)
#     target/release/bundle/osx/<Name>.app                       (plain `cargo bundle --release`)
#
# Icon source, the first one found in apps/<app>/assets/:
#   1. AppIcon.icon/          Icon Composer document (macOS 26 format; needs actool from Xcode 26)
#   2. AppIcon.appiconset/    asset catalog icon set: PNGs plus Contents.json
#   3. icon.icns              the existing icon; every size is taken from it or scaled down from
#                             its largest image
# Without any of them the script says so and exits 0, leaving the bundle as cargo-bundle made it.
#
# Run it BEFORE scripts/sign_app.sh: changing the bundle after signing breaks the signature.
# macOS only: xcrun actool (Xcode or the Command Line Tools), plus iconutil and sips for icon.icns.
# The deployment target comes from minimum_system_version in apps/<app>/Cargo.toml.
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
die() { echo -e "${RED}$*${NC}" >&2; exit 1; }
note() { echo -e "${YELLOW}$*${NC}" >&2; }

ICON_NAME="AppIcon"
PLISTBUDDY="${PLISTBUDDY:-/usr/libexec/PlistBuddy}"

PKG="${1:-}"
[ -n "$PKG" ] || die "Gebruik: $0 <copycraft|ticker> [pad/naar/App.app]"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOML="$ROOT/apps/$PKG/Cargo.toml"
ASSETS="$ROOT/apps/$PKG/assets"
[ -f "$TOML" ] || die "Geen $TOML"

# 1. Icon source. Nothing to compile is not an error.
SOURCE=""
KIND=""
if [ -d "$ASSETS/$ICON_NAME.icon" ]; then
  SOURCE="$ASSETS/$ICON_NAME.icon"; KIND="icon"
elif [ -d "$ASSETS/$ICON_NAME.appiconset" ]; then
  SOURCE="$ASSETS/$ICON_NAME.appiconset"; KIND="appiconset"
elif [ -f "$ASSETS/icon.icns" ]; then
  SOURCE="$ASSETS/icon.icns"; KIND="icns"
else
  note "⏭️  Geen app-icoonbron in $ASSETS ($ICON_NAME.icon, $ICON_NAME.appiconset of icon.icns): overgeslagen."
  exit 0
fi

# 2. The bundle, found like scripts/sign_app.sh does.
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
RES="$APP/Contents/Resources"
PLIST="$APP/Contents/Info.plist"
[ -f "$PLIST" ] || die "Geen $PLIST: is $APP een app-bundle?"
mkdir -p "$RES"

MIN_OS="$(awk '
  /^\[package\.metadata\.bundle\.macos\][[:space:]]*$/ { s = 1; next }
  /^\[/ { s = 0 }
  s && /^minimum_system_version[[:space:]]*=/ { sub(/^[^"]*"/, ""); sub(/".*$/, ""); print; exit }
' "$TOML")"
MIN_OS="${MIN_OS:-11.0}"

# 3. Tools.
command -v xcrun >/dev/null || die "xcrun niet gevonden: dit script draait alleen op macOS met Xcode of de Command Line Tools"
xcrun --find actool >/dev/null 2>&1 || die "actool niet gevonden (xcrun --find actool). Installeer Xcode en kies het met xcode-select."
[ -x "$PLISTBUDDY" ] || die "PlistBuddy niet gevonden op $PLISTBUDDY"
command -v plutil >/dev/null || die "plutil niet gevonden"
if [ "$KIND" = "icns" ]; then
  command -v iconutil >/dev/null || die "iconutil niet gevonden"
  command -v sips >/dev/null || die "sips niet gevonden"
fi
if [ "$KIND" = "icon" ]; then
  ACTOOL_VERSION="$(xcrun actool --version --output-format human-readable-text 2>/dev/null \
    | sed -n 's/^[[:space:]]*short-bundle-version:[[:space:]]*\([0-9][0-9.]*\).*/\1/p' | head -n 1)"
  if [ -z "$ACTOOL_VERSION" ]; then
    note "⚠️  actool-versie onbekend; een .icon vraagt actool uit Xcode 26 of nieuwer."
  elif [ "${ACTOOL_VERSION%%.*}" -lt 26 ]; then
    die "$ICON_NAME.icon vraagt actool uit Xcode 26 of nieuwer, gevonden: $ACTOOL_VERSION. Kies Xcode 26 met xcode-select of DEVELOPER_DIR."
  fi
fi

WORK="$(mktemp -d "${TMPDIR:-/tmp}/$PKG-app-icon.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

# 4. actool input: the .icon itself, or an asset catalog holding the icon set.
CATALOG="$WORK/Icon.xcassets"
SET="$CATALOG/$ICON_NAME.appiconset"
write_catalog_contents() {
  mkdir -p "$CATALOG"
  printf '{\n  "info" : {\n    "author" : "xcode",\n    "version" : 1\n  }\n}\n' > "$CATALOG/Contents.json"
}

case "$KIND" in
  icon)
    INPUT="$SOURCE"
    ;;
  appiconset)
    write_catalog_contents
    cp -R "$SOURCE" "$SET"
    [ -f "$SET/Contents.json" ] || die "Geen Contents.json in $SOURCE"
    INPUT="$CATALOG"
    ;;
  icns)
    write_catalog_contents
    mkdir -p "$SET"
    ICONSET="$WORK/extracted.iconset"
    iconutil -c iconset "$SOURCE" -o "$ICONSET" || die "iconutil kon $SOURCE niet uitpakken"
    pixel_width() { sips -g pixelWidth "$1" 2>/dev/null | awk '/pixelWidth/ { print $2 }'; }
    LARGEST=""
    LARGEST_PX=0
    for png in "$ICONSET"/*.png; do
      [ -f "$png" ] || continue
      px="$(pixel_width "$png")"
      if [ -n "$px" ] && [ "$px" -gt "$LARGEST_PX" ]; then
        LARGEST="$png"; LARGEST_PX="$px"
      fi
    done
    [ -n "$LARGEST" ] || die "Geen PNG-afbeelding in $SOURCE"
    [ "$LARGEST_PX" -ge 512 ] || note "⚠️  Grootste afbeelding in $SOURCE is ${LARGEST_PX}px; grotere maten worden opgeschaald."
    ENTRIES=()
    for size in 16 32 128 256 512; do
      for scale in 1 2; do
        px=$((size * scale))
        suffix=""
        [ "$scale" = 2 ] && suffix="@2x"
        file="icon_${size}x${size}${suffix}.png"
        if [ -f "$ICONSET/$file" ] && [ "$(pixel_width "$ICONSET/$file")" = "$px" ]; then
          cp "$ICONSET/$file" "$SET/$file"
        else
          sips -s format png -z "$px" "$px" "$LARGEST" --out "$SET/$file" >/dev/null \
            || die "sips kon $file niet maken uit $LARGEST"
        fi
        ENTRIES+=("    {\n      \"filename\" : \"$file\",\n      \"idiom\" : \"mac\",\n      \"scale\" : \"${scale}x\",\n      \"size\" : \"${size}x${size}\"\n    }")
      done
    done
    {
      printf '{\n  "images" : [\n'
      for i in "${!ENTRIES[@]}"; do
        [ "$i" -gt 0 ] && printf ',\n'
        printf '%b' "${ENTRIES[$i]}"
      done
      printf '\n  ],\n  "info" : {\n    "author" : "xcode",\n    "version" : 1\n  }\n}\n'
    } > "$SET/Contents.json"
    INPUT="$CATALOG"
    ;;
esac

# 5. Compile into a scratch folder first, so a failure leaves the bundle untouched.
OUT="$WORK/out"
mkdir -p "$OUT"
echo -e "${YELLOW}🎨 $NAME: app-icoon uit $(basename "$SOURCE") ($KIND), macOS $MIN_OS+${NC}"
xcrun actool "$INPUT" \
  --compile "$OUT" \
  --platform macosx \
  --target-device mac \
  --minimum-deployment-target "$MIN_OS" \
  --app-icon "$ICON_NAME" \
  --include-all-app-icons \
  --enable-on-demand-resources NO \
  --development-region en \
  --output-partial-info-plist "$WORK/partial.plist" \
  --output-format human-readable-text --notices --warnings --errors \
  || die "actool faalde voor $SOURCE (zie de meldingen hierboven)"
[ -f "$OUT/Assets.car" ] || die "actool maakte geen Assets.car uit $SOURCE"
[ -f "$OUT/$ICON_NAME.icns" ] || die "actool maakte geen $ICON_NAME.icns uit $SOURCE"

cp "$OUT/Assets.car" "$RES/Assets.car"
cp "$OUT/$ICON_NAME.icns" "$RES/$ICON_NAME.icns"

# 6. Info.plist: the catalog icon on macOS 26 (and 11+), the .icns for Finder and older systems.
plist_set() {
  "$PLISTBUDDY" -c "Set :$1 $2" "$PLIST" 2>/dev/null || "$PLISTBUDDY" -c "Add :$1 string $2" "$PLIST"
}
plist_set CFBundleIconName "$ICON_NAME"
plist_set CFBundleIconFile "$ICON_NAME"
plutil -lint "$PLIST" >/dev/null || die "$PLIST is ongeldig na het bijwerken"
[ "$("$PLISTBUDDY" -c 'Print :CFBundleIconName' "$PLIST")" = "$ICON_NAME" ] \
  || die "CFBundleIconName staat niet op $ICON_NAME in $PLIST"

echo "=== Resources ==="
ls -la "$RES/Assets.car" "$RES/$ICON_NAME.icns"
echo -e "${GREEN}✅ $NAME.app: Assets.car en $ICON_NAME.icns geplaatst, CFBundleIconName/CFBundleIconFile = $ICON_NAME${NC}"
