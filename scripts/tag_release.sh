#!/bin/bash
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
BRANCH="${BRANCH:-main}"
APP="${1:-}"

cd "$(git rev-parse --show-toplevel)"
[ -n "$APP" ] || { echo -e "${RED}Gebruik: $0 <copycraft|ticker>${NC}"; exit 1; }
TOML="apps/$APP/Cargo.toml"
[ -f "$TOML" ] || { echo -e "${RED}Geen $TOML${NC}"; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo -e "${RED}Werkmap niet schoon${NC}"; exit 1; }

git switch "$BRANCH"
git pull --ff-only

CURRENT=$(grep -m1 '^version = ' "$TOML" | sed -E 's/version = "(.*)"/\1/')
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"
NEW="$MAJOR.$MINOR.$((PATCH + 1))"
TAG="$APP-v$NEW"

echo -e "${YELLOW}🚀 $APP release: ${GREEN}$CURRENT → $NEW${NC}"
read -p "Doorgaan? (y/n) " -n 1 -r; echo
[[ $REPLY =~ ^[Yy]$ ]] || { echo -e "${RED}Geannuleerd${NC}"; exit 1; }

sed -i '' "s/^version = \"$CURRENT\"/version = \"$NEW\"/" "$TOML"
sed -i '' "s/version = \"$CURRENT\" # bundle/version = \"$NEW\" # bundle/" "$TOML"
grep -q "^version = \"$NEW\"" "$TOML" || { echo -e "${RED}Versie niet bijgewerkt${NC}"; exit 1; }

cargo update -p "$APP" --offline 2>/dev/null || cargo metadata --format-version 1 >/dev/null

git add "$TOML" Cargo.lock
git commit -m "$APP: bump version to $NEW"
git tag "$TAG"
git push origin "$BRANCH"
git push origin "$TAG"

echo -e "${GREEN}✅ $APP $NEW gereleased, GitHub Actions bouwt de DMG.${NC}"
