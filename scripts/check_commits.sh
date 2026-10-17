#!/usr/bin/env bash
# Check that every non-merge commit in BASE..HEAD has a Conventional Commits subject.
#   scripts/check_commits.sh BASE HEAD
# Commits at or before START (and everything in its ancestry) are never checked.
set -euo pipefail
if [ $# -ne 2 ]; then
  echo "usage: $0 BASE HEAD" >&2
  exit 2
fi
pattern='^(feat|fix|refactor|docs|test|ci|chore|build|perf|style|revert)(\([a-z0-9._-]+\))?!?: .+'
START=f291f37a4dc0f68df111641b31221c3600a9d221
if ! git cat-file -e "$START^{commit}" 2>/dev/null; then
  echo "error: start commit $START not found; fetch full history (actions/checkout fetch-depth: 0)" >&2
  exit 2
fi
bad=0
while IFS= read -r line; do
  sha=${line%% *}
  subject=${line#* }
  if ! [[ $subject =~ $pattern ]]; then
    echo "not a Conventional Commit: $sha $subject" >&2
    bad=1
  fi
done < <(git log --no-merges --format='%H %s' "$1..$2" "^$START")
if [ "$bad" -ne 0 ]; then
  echo "See CONTRIBUTING.md for the format." >&2
fi
exit "$bad"
