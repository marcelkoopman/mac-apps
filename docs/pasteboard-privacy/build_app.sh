#!/bin/bash
# Build PasteProbe.app from PasteProbe.swift and sign it.
#   ./build_app.sh                       ad-hoc (codesign -s -), like copycraft's releases now
#   SIGN_ID="Developer ID Application: …" ./build_app.sh   stable identity (Developer ID)
#   SIGN_ID="PasteProbe Test" ./build_app.sh               or a self-signed certificate
#   BUNDLE_ID=… AGENT=1 ./build_app.sh   other bundle id / LSUIElement app (no Dock icon, as copycraft)
set -euo pipefail
cd "$(dirname "$0")"

SIGN_ID="${SIGN_ID:--}"
BUNDLE_ID="${BUNDLE_ID:-nl.marcelkoopman.pasteprobe}"
AGENT="${AGENT:-0}"
APP="build/PasteProbe.app"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
xcrun swiftc -O -o "$APP/Contents/MacOS/PasteProbe" PasteProbe.swift

UI_ELEMENT="<false/>"
[ "$AGENT" = "1" ] && UI_ELEMENT="<true/>"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
  <key>CFBundleName</key><string>PasteProbe</string>
  <key>CFBundleExecutable</key><string>PasteProbe</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
  <key>CFBundleVersion</key><string>$(date +%Y%m%d%H%M%S)</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSUIElement</key>${UI_ELEMENT}
</dict>
</plist>
PLIST

if [ "$SIGN_ID" = "-" ]; then
  codesign --force --sign - "$APP"
else
  codesign --force --options runtime --timestamp --sign "$SIGN_ID" "$APP"
fi

echo "--- signature"
codesign -dv "$APP" 2>&1 | grep -E "^(Identifier|CDHash|TeamIdentifier|Signature|Authority)" || true
echo "--- designated requirement (what TCC remembers the app by)"
codesign -dr - "$APP" 2>&1 | tail -n 1
echo
echo "Built $APP"
echo "Run:  open $APP --args [--window] [--no-read] ..."
echo "Log:  tail -f ~/Library/Logs/PasteProbe.log"
