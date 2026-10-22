#!/bin/bash
# Release notes input for `gh release create` in .github/workflows/release-<app>.yml.
#
#   scripts/release_notes.sh <app> <tag> <notes-out>
#
# Prints the previous tag of the same app (`<app>-v*`, by version, the one just before <tag>),
# so GitHub's generated notes compare with that app's own last release and never with another
# app's tag (a ticker tag for copycraft, or the other way round). Prints nothing for the
# first release.
#
# Writes apps/<app>/RELEASE_NOTES_NEXT.md to <notes-out> when that file changed since the
# previous tag (so a note ships once, in the first release after it was written); otherwise
# <notes-out> is left empty. Needs the full history and tags (checkout fetch-depth: 0).
set -euo pipefail

APP="${1:?app}"
TAG="${2:?tag}"
OUT="${3:?notes file}"
NOTES="apps/$APP/RELEASE_NOTES_NEXT.md"

case "$TAG" in
    "$APP"-v*) ;;
    *) echo "release_notes.sh: $TAG is not a $APP-v* tag" >&2; exit 1 ;;
esac

PREV=$(git tag --list "$APP-v*" --sort=v:refname \
    | awk -v cur="$TAG" '$0 == cur { print prev; exit } { prev = $0 }')

: > "$OUT"
if [ -f "$NOTES" ] && { [ -z "$PREV" ] || ! git diff --quiet "$PREV" "$TAG" -- "$NOTES"; }; then
    # Drop the HTML comment block (instructions for whoever edits the file).
    awk '/^<!--/ { skip = 1 } !skip { print } /-->/ { skip = 0 }' "$NOTES" > "$OUT"
fi
echo "$PREV"
