# copycraft

macOS clipboard command. ⌃⌥⌘F opens a card at the cursor for what you copied. Type to search. The menu bar icon shows that Copycraft is running.

## Downloads

Klaar-voor-gebruik builds staan op de [Releases](https://github.com/marcelkoopman/copycraft/releases)-pagina (signed `.app` + `.dmg`).

## Pipeline

Same layout as [ticker](https://github.com/marcelkoopman/ticker):

- `Rust CI` on push/PR: fmt, clippy, tests, unused-deps (`cargo machete`)
- Weekly + manual: coverage and `cargo audit`
- Tag `v*` → signed `.app` + `Copycraft-vX.Y.Z.dmg` GitHub Release

```bash
# local quality
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test

# local DMG (needs cargo-bundle + create-dmg)
./build-local-release.sh

# bump patch, tag, push (triggers the DMG workflow)
./tag_release.sh

# optional local hooks
./scripts/install-git-hooks.sh
```
