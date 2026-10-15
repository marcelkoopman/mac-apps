#!/usr/bin/env bash
# Guards for copycraft's offline rule (AGENTS.md): fails when
#   1. copycraft's normal dependency tree for aarch64-apple-darwin holds an HTTP, TLS, WebSocket
#      or socket crate (reqwest, hyper, ureq, curl, isahc, rustls, native-tls, openssl,
#      tungstenite, socket2, or a crate named after one, such as hyper-util or openssl-sys);
#   2. copycraft's sources mention osascript;
#   3. the shared layer (crates/mac-ui/src) starts a subprocess or uses std::net /
#      std::os::unix::net: with feature unification in workspace builds, whatever mac-ui holds
#      can end up in copycraft. Ticker keeps its own subprocess code (its osascript fallback).
#   4. an old bundle id (without the mac-apps. segment) is still in the repo, or the ids in
#      Cargo.toml / capture::OWN_BUNDLE differ from the expected ones.
# In copycraft itself, subprocesses and std::net / std::os::unix::net are caught by clippy
# (apps/copycraft/clippy.toml).
#
# Allowlisted: socket2, only as a dependency of tokio, and tokio only as a dependency of polars
# crates. polars-core and polars-async (0.55) turn on tokio's "net" feature unconditionally (no
# polars feature turns it off), which pulls in socket2 and mio. Copycraft never uses tokio's net
# types; polars uses tokio for its async runtime (and object stores copycraft does not enable).
#
# Usage: scripts/check_copycraft_offline.sh   (from anywhere; needs cargo, works on Linux too)
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; NC='\033[0m'
die() { echo -e "${RED}$*${NC}" >&2; exit 1; }

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TARGET=aarch64-apple-darwin
tree() { cargo tree --locked -p copycraft -e normal --target "$TARGET" --prefix none "$@"; }

DENY='^(reqwest|hyper|ureq|curl|isahc|rustls|native-tls|openssl|socket2)(-[a-z0-9-]+)?$|tungstenite'
crates="$(tree | awk '{print $1}' | sort -u)"
found="$(grep -E "$DENY" <<<"$crates" || true)"

if grep -qx socket2 <<<"$found"; then
  # Who depends on socket2, and on tokio, one level up (the first line is the crate itself).
  socket2_users="$(tree -i socket2 --depth 1 | awk 'NR > 1 {print $1}' | sort -u)"
  tokio_users="$(tree -i tokio --depth 1 | awk 'NR > 1 {print $1}' | sort -u)"
  if [ "$socket2_users" = "tokio" ] && ! grep -qv '^polars-' <<<"$tokio_users"; then
    found="$(grep -vx socket2 <<<"$found" || true)"
  else
    echo "socket2 is used by: $socket2_users; tokio by: $tokio_users" >&2
  fi
fi

[ -z "$found" ] || die "Netwerk-crates in de dependency tree van copycraft ($TARGET):
$found
Copycraft is offline (AGENTS.md). Zie: cargo tree -p copycraft -e normal --target $TARGET -i <crate>"

if grep -rn osascript apps/copycraft/src apps/copycraft/Cargo.toml; then
  die "osascript in copycraft: dialogen en panels zijn native in-process (AGENTS.md)"
fi

if grep -rnE 'process::Command|Command::new|std::net|os::unix::net|TcpStream|UdpSocket|UnixStream' crates/mac-ui/src; then
  die "Subprocess- of netwerkcode in crates/mac-ui: de gedeelde laag heeft die niet, die hoort in de app zelf (AGENTS.md)"
fi

# mac-ai (local AI through Foundation Models, a dependency of copycraft) is offline too: no
# network or subprocess code in its sources (build.rs may start swiftc at build time).
if grep -rnE 'process::Command|Command::new|std::net|os::unix::net|TcpStream|UdpSocket|UnixStream' crates/mac-ai/src crates/mac-ai/swift; then
  die "Subprocess- of netwerkcode in crates/mac-ai: de AI-laag draait lokaal en offline (AGENTS.md)"
fi
if grep -rnE 'URLSession|NWConnection|URLRequest|import Network' crates/mac-ai/swift; then
  die "Netwerkcode in de Swift-bridge van crates/mac-ai (AGENTS.md)"
fi

# Bundle ids: copycraft and ticker moved to com.github.marcelkoopman.mac-apps.<app> before the
# first Store submission. The old ids (no "mac-apps." segment) must not come back anywhere in the
# tracked files, and the id in Cargo.toml, capture::OWN_BUNDLE and the expected value must agree.
old_ids='com\.github\.marcelkoopman\.(copycraft|ticker)'
# docs/pasteboard-privacy/FINDINGS.md is historical (test logs) and may keep an old id.
if git grep -nIE "$old_ids" -- . ':!docs/pasteboard-privacy/FINDINGS.md'; then
  die "Oude bundle id in de repo (nu com.github.marcelkoopman.mac-apps.<app>); zie hierboven"
fi
for app in copycraft ticker; do
  want="com.github.marcelkoopman.mac-apps.$app"
  have="$(sed -n 's/^identifier *= *"\(.*\)"/\1/p' "apps/$app/Cargo.toml")"
  [ "$have" = "$want" ] || die "apps/$app/Cargo.toml: identifier is '$have', verwacht '$want'"
done
grep -q 'OWN_BUNDLE: &str = "com.github.marcelkoopman.mac-apps.copycraft";' apps/copycraft/src/capture.rs \
  || die "capture::OWN_BUNDLE komt niet overeen met de bundle id in apps/copycraft/Cargo.toml"

echo -e "${GREEN}copycraft: geen netwerk-crates (socket2 via polars → tokio toegestaan), geen osascript; mac-ui: geen subprocessen of netwerk${NC}"
