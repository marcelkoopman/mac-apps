#!/usr/bin/env bash
# Build a Mac App Store installer package of Copycraft (maintainer only, macOS only; NOT run in CI).
#
#   scripts/build_store_pkg.sh [output.pkg]       default: target/store/Copycraft.pkg
#
# Steps: release build for aarch64-apple-darwin, cargo bundle, Info.plist finish, app icon,
# privacy manifest check, embed the provisioning profile as Contents/embedded.provisionprofile,
# codesign with the Apple Distribution identity and apps/copycraft/assets/entitlements.store.plist
# (TEAMID filled in), then `productbuild --sign` with the installer identity. Uploading the .pkg
# (Transporter) is not part of this script; App Store builds are not notarized.
#
# Required environment (nothing is read from the repo or a secret store):
#   APPLE_TEAM_ID              10-character team id, e.g. ABCDE12345
#   STORE_APP_IDENTITY         "Apple Distribution: Name (TEAMID)" (older name:
#                              "3rd Party Mac Developer Application: Name (TEAMID)")
#   STORE_INSTALLER_IDENTITY   "3rd Party Mac Developer Installer: Name (TEAMID)" (shown as
#                              "Mac Installer Distribution" in the developer portal)
#   STORE_PROVISION_PROFILE    path to the Mac App Store provisioning profile (.provisionprofile)
#                              for nl.marcelkoopman.copycraft
# Optional:
#   KEYCHAIN                   limit the identity lookup and signing to this keychain
#
# The script stops with a clear message when an identity is not in the keychain
# (`security find-identity`), the profile is missing, or the profile is for another team or app.
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
die() { echo -e "${RED}build_store_pkg: $*${NC}" >&2; exit 1; }

PKG=copycraft
BUNDLE_ID=nl.marcelkoopman.copycraft
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="$ROOT/apps/$PKG"
TOML="$APP_DIR/Cargo.toml"
ENTITLEMENTS_SRC="$APP_DIR/assets/entitlements.store.plist"
OUT="${1:-$ROOT/target/store/Copycraft.pkg}"

[ "$(uname -s)" = Darwin ] || die "alleen op macOS (codesign, security, productbuild)"
for tool in codesign security productbuild plutil cargo; do
  command -v "$tool" >/dev/null || die "$tool niet gevonden"
done
cargo bundle --help >/dev/null 2>&1 || die "cargo-bundle ontbreekt: cargo install cargo-bundle"
[ -f "$ENTITLEMENTS_SRC" ] || die "geen $ENTITLEMENTS_SRC"

# 1. Inputs, before anything is built.
TEAM="${APPLE_TEAM_ID:-}"
APP_IDENTITY="${STORE_APP_IDENTITY:-}"
INSTALLER_IDENTITY="${STORE_INSTALLER_IDENTITY:-}"
PROFILE="${STORE_PROVISION_PROFILE:-}"
[ -n "$TEAM" ] || die "APPLE_TEAM_ID is niet gezet (10 tekens, zie developer.apple.com › Membership)"
[[ "$TEAM" =~ ^[A-Z0-9]{10}$ ]] || die "APPLE_TEAM_ID '$TEAM' is geen team id van 10 tekens (A-Z, 0-9)"
[ -n "$APP_IDENTITY" ] || die "STORE_APP_IDENTITY is niet gezet, bijv. \"Apple Distribution: Naam ($TEAM)\""
[ -n "$INSTALLER_IDENTITY" ] || die "STORE_INSTALLER_IDENTITY is niet gezet, bijv. \"3rd Party Mac Developer Installer: Naam ($TEAM)\""
[ -n "$PROFILE" ] || die "STORE_PROVISION_PROFILE is niet gezet (pad naar het .provisionprofile van $BUNDLE_ID)"
[ -f "$PROFILE" ] || die "provisioning profile niet gevonden: $PROFILE"

KEYCHAIN_ARGS=()
if [ -n "${KEYCHAIN:-}" ]; then
  [ -f "$KEYCHAIN" ] || die "KEYCHAIN '$KEYCHAIN' bestaat niet"
  KEYCHAIN_ARGS=(--keychain "$KEYCHAIN")
fi

# Identities: the application certificate must be valid for code signing; an installer
# certificate is not a code-signing identity, so it is looked up without that policy.
has_identity() { # <name> [policy args...]
  local name="$1"; shift
  security find-identity -v "$@" ${KEYCHAIN:+"$KEYCHAIN"} | grep -F -q "\"$name\""
}
has_identity "$APP_IDENTITY" -p codesigning \
  || die "identiteit \"$APP_IDENTITY\" niet in de keychain (security find-identity -v -p codesigning). Installeer het Apple Distribution-certificaat met private key."
has_identity "$INSTALLER_IDENTITY" \
  || die "identiteit \"$INSTALLER_IDENTITY\" niet in de keychain (security find-identity -v). Installeer het Mac Installer Distribution-certificaat met private key."

# Profile: decode the CMS envelope and check team and app id.
WORK="$(mktemp -d "${TMPDIR:-/tmp}/copycraft-store.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
security cms -D -i "$PROFILE" -o "$WORK/profile.plist" 2>/dev/null \
  || die "kan $PROFILE niet lezen (security cms -D); is het een .provisionprofile?"
profile_value() { /usr/libexec/PlistBuddy -c "Print :$1" "$WORK/profile.plist" 2>/dev/null || true; }
[ "$(profile_value 'TeamIdentifier:0')" = "$TEAM" ] \
  || die "profiel is voor team '$(profile_value 'TeamIdentifier:0')', niet voor APPLE_TEAM_ID '$TEAM'"
[ "$(profile_value 'Entitlements:com.apple.application-identifier')" = "$TEAM.$BUNDLE_ID" ] \
  || die "profiel is voor '$(profile_value 'Entitlements:com.apple.application-identifier')', verwacht '$TEAM.$BUNDLE_ID'"

# Entitlements with the team id filled in; no placeholder may be left.
ENTITLEMENTS="$WORK/entitlements.plist"
sed "s/TEAMID/$TEAM/g" "$ENTITLEMENTS_SRC" > "$ENTITLEMENTS"
plutil -lint "$ENTITLEMENTS" >/dev/null || die "entitlements parsen niet"
if grep -q 'TEAMID' "$ENTITLEMENTS"; then die "placeholder TEAMID staat nog in de entitlements"; fi

# 2. Build and bundle (same steps as .github/workflows/release-copycraft.yml).
echo -e "${YELLOW}🔨 Release build${NC}"
(cd "$ROOT" && cargo build --release --locked --target aarch64-apple-darwin -p "$PKG")
"$ROOT/scripts/check_minos.sh" "$ROOT/target/aarch64-apple-darwin/release/$PKG"
(cd "$APP_DIR" && cargo bundle --release --target aarch64-apple-darwin)

NAME="$(awk '
  /^\[package\.metadata\.bundle\][[:space:]]*$/ { s = 1; next }
  /^\[/ { s = 0 }
  s && /^name[[:space:]]*=/ { sub(/^name[[:space:]]*=[[:space:]]*"/, ""); sub(/".*$/, ""); print; exit }
' "$TOML")"
APP="$ROOT/target/aarch64-apple-darwin/release/bundle/osx/$NAME.app"
[ -d "$APP" ] || die "geen $APP na cargo bundle"

"$ROOT/scripts/finish_info_plist.sh" "$PKG" "$APP"
"$ROOT/scripts/build_app_icon.sh" "$PKG" "$APP"
"$ROOT/scripts/check_privacy_manifest.sh" "$PKG" "$APP"

# 3. Embed the profile and sign. No --deep: one executable, no nested code.
cp "$PROFILE" "$APP/Contents/embedded.provisionprofile"
echo -e "${YELLOW}🔏 Signing $APP ($APP_IDENTITY)${NC}"
codesign --force --sign "$APP_IDENTITY" "${KEYCHAIN_ARGS[@]+"${KEYCHAIN_ARGS[@]}"}" \
  --options runtime --timestamp --entitlements "$ENTITLEMENTS" "$APP"
codesign --verify --strict --verbose=2 "$APP"
SIGNED="$(codesign -d --entitlements - --xml "$APP" 2>/dev/null)"
for key in com.apple.security.app-sandbox com.apple.security.files.user-selected.read-write \
  com.apple.application-identifier com.apple.developer.team-identifier; do
  grep -q "$key" <<<"$SIGNED" || die "entitlement $key ontbreekt in de signature"
done
if grep -q 'network\.' <<<"$SIGNED"; then die "netwerk-entitlement in de signature (copycraft is offline)"; fi

# 4. Installer package.
mkdir -p "$(dirname "$OUT")"
echo -e "${YELLOW}📦 productbuild → $OUT${NC}"
productbuild --component "$APP" /Applications --sign "$INSTALLER_IDENTITY" \
  "${KEYCHAIN_ARGS[@]+"${KEYCHAIN_ARGS[@]}"}" "$OUT"
pkgutil --check-signature "$OUT" >/dev/null || die "pkgutil --check-signature faalt voor $OUT"

echo -e "${GREEN}✅ $OUT (team $TEAM, $BUNDLE_ID). Upload met Transporter.${NC}"
