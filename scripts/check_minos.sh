#!/usr/bin/env bash
# Fails unless every given Mach-O binary has minos 26.0 (LC_BUILD_VERSION), as read by `otool -l`.
# Usage: scripts/check_minos.sh <binary>...
set -euo pipefail
EXPECTED="${EXPECTED_MINOS:-26.0}"
[ "$#" -gt 0 ] || { echo "usage: $0 <binary>..." >&2; exit 2; }
status=0
for bin in "$@"; do
  [ -f "$bin" ] || { echo "::error::$bin not found"; status=1; continue; }
  minos="$(otool -l "$bin" | awk '/cmd LC_BUILD_VERSION/ { s = 1 } s && $1 == "minos" { print $2; exit }')"
  if [ -z "$minos" ]; then
    minos="$(otool -l "$bin" | awk '/cmd LC_VERSION_MIN_MACOSX/ { s = 1 } s && $1 == "version" { print $2; exit }')"
  fi
  if [ "$minos" = "$EXPECTED" ]; then
    echo "ok: $bin minos $minos"
  else
    echo "::error::$bin has minos '${minos:-none}', expected $EXPECTED"
    status=1
  fi
done
exit "$status"
