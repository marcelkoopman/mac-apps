# Copycraft

A clipboard command for macOS. Press **⌃⌥⌘C** and a card opens at the cursor showing what you copied, plus the actions that fit it. The menu bar icon shows that Copycraft is running: a monochrome template that follows the menu bar (light, dark or tinted) and blinks briefly on each new copy. The detected kind (JSON, Rust, Image, a link's host, YouTube, …) shows in its tooltip and its VoiceOver label. The app has no Dock icon.

Part of the [mac-apps](../../README.md) monorepo.

## Features

- **Detection**: JSON, YAML, XML, HTML, Markdown, Rust, Java, Python, URLs, CSV/TSV, plain text and images. JSON and XML get a *Valid* / *Invalid* title. A page with `<!DOCTYPE html>` or a root `<html>` is *HTML*: highlighted and formatted like XML, without the XML check or a Schema chip. Rust and Java whose brackets do not balance (strings, char literals and comments ignored) get a title such as *Java · missing }* or *Rust · unbalanced )*; formatting still runs and adds no brackets. Python is recognised by a score of Python-only hints (`def`/`class …:`, `import`, `from … import`, `elif`, `with open(`, `self`, `None`/`True`/`False`, an indented block after `:`, `print(`, f-strings, `#` comments) with at least one strong hint, so a lone line such as `f = open("demofile.txt")` stays plain text; Ruby, JavaScript, YAML and prose do not qualify. Python is highlighted but not reformatted, and its title flags brackets (*Python · missing )*), a block without an indented body (*Python · expected indent*) and *mixed tabs and spaces*.
- **Format**: pretty-print with syntax highlighting. JSON, YAML, XML, HTML, Markdown, Rust and Java open already formatted.
- **Convert**: JSON ↔ YAML, CSV → JSON, TSV → CSV, flat `key: value` text → JSON.
- **Decode**: JWT, Base64, `data:` URIs, percent-encoding.
- **Dataframe**: CSV, TSV or JSON as a table (Polars). Saving from this view writes Parquet.
- **Schema / Sample**: JSON → Avro schema (`.avsc`), XML → XSD, XSD → sample XML.
- **Images**: *Info* shows format, size and a data URL. *Text* runs OCR and *QR* reads barcode payloads, both with Apple Vision.
- **Links**: *Visit* opens the URL in your browser. The card loads the page title and preview image, or for YouTube the thumbnail and title. Only the link on the card shown is fetched, never history entries around it. No preview is fetched for a URL with `user:password@`, for local or LAN hosts (`localhost`, `*.local`, private and loopback addresses, single-label names) or for token-like query parameters (`token=`, `code=`, signatures, keys, JWTs); the page's preview image gets the same checks. Only 2xx responses up to 5 MB are used. *Visit* opens only `http`/`https` links (through `NSWorkspace`), with `user:password@` stripped.
- **Copy / Save**: copy the current view back to the clipboard, or save it through a save panel. The file extension follows the content.
- **Choose file**: show a local text file (up to 8 MB) on the card instead of the clipboard.
- **Drop**: drop one text file, or text, on the card to show it the same way (up to 8 MB). A drop goes into history like a copy, without changing the clipboard. The card stays open while you work in another app, so open it, then drag from Finder, Safari or Notes onto it.
- **History**: the last 20 copies and drops (text and images), kept in memory only. Step through them with the ‹ / › chevrons in the glass capsule at the right of the chip row, which shows where you are (*1 / 5*; 1 is the newest copy, as in the *History* submenu; › *Next* counts up to older copies, ‹ *Previous* counts back down toward 1), or use the menu bar *History* submenu. The capsule appears once history holds two copies.

## Privacy

- **Masked content**: the card blurs copied content until you click it.
- **Sensitivity labels**: the meta line flags **credential**, **PII** and **financial** content, using [leakguard](https://crates.io/crates/leakguard) (tokens, keys, email, IBAN, cards, and more) and [redact-core](https://crates.io/crates/redact-core) with Dutch field recognizers (labeled or tabular fields such as names and salaries, NL phone numbers, BSN). Copycraft only labels content. It never rewrites it.
- **Wipe**: the *Wipe* button clears history, caches, the card and the pasteboard. Copied text and image buffers are zeroized when they are dropped (`zeroize`).
- **Masked again on focus loss**: the card stays open (floating) when another app becomes active, but content you revealed is blurred again. It closes with ✕, Esc, the hotkey or a click on the menu bar icon.

## Usage

| Input | Action |
| --- | --- |
| ⌃⌥⌘C | Open the card at the cursor, or close it |
| Click the menu bar icon | Open the card under the icon, or close it |
| Right-click the menu bar icon | Hotkey hint, *History*, version, *Quit* |
| Type any text, or `/` | Search actions, history and appearance |
| ← → ↑ ↓, Return | Move the selection, run the selected action |
| ⌘F | Find in the revealed content. Return / ⇧Return jump to the next / previous match |
| Esc | Clear the find or search field, then close the card |
| `⋯` | Choose file, Clipboard, Empty pasteboard, history, Clear history, Appearance (System / Light / Dark), Quit |

## Build and run

From the repo root:

```bash
cargo run -p copycraft
cargo test -p copycraft
cargo clippy -p copycraft --all-targets -- -D warnings
```

Local app bundle (needs `cargo-bundle`):

```bash
cd apps/copycraft && cargo bundle --release
```

## Release

```bash
scripts/tag_release.sh copycraft
```

This bumps the patch version and tags `copycraft-vX.Y.Z`. GitHub Actions then builds an ad-hoc signed `copycraft-X.Y.Z.dmg` (Apple silicon) and publishes it on [Releases](https://github.com/marcelkoopman/mac-apps/releases). See the [root README](../../README.md#release) for details.

## License

Apache-2.0, see [LICENSE](LICENSE).
