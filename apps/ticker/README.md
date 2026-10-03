# Ticker

A menu bar price ticker for macOS. It shows one asset's price in the menu bar and lists every configured asset in the menu. The default assets are Bitcoin, ETH, gold, TTF gas, NL petrol/diesel and NL power. The app has no Dock icon.

Part of the [mac-apps](../../README.md) monorepo.

## Features

- **Polling**: prices refresh every 5 minutes, or on demand with *Poll now*. Each URL is fetched once per poll (assets that share a URL, like petrol and diesel, share the request) and tried up to 3 times; until a fetch succeeds the menu keeps the last known price. Failed attempts are written to the debug log. Redirects are followed at most 3 times and only to `https://` URLs.
- **Freshness**: each row's second line says when its price was last fetched (`updated 14:05`). A price that failed 2 polls in a row or is older than 15 minutes is stale: its row turns grey with `⚠︎` in front, and so does the menu bar title when it shows that asset.
- **Day change**: each menu row shows the price with its unit and the change since the day's first price, e.g. `▲ €217,00 · +0.33% · updated 14:05`. That first price is stored per local calendar day. A move under 0.05% of the earlier price counts as flat and shows no arrow.
- **Menu bar asset**: click a price row to show that asset in the menu bar. The choice is remembered.
- **Price watches**: *Add Price Watch* takes an asset and a target price (prefilled with the current price, in the asset's unit). The direction follows from the current price: a higher target is an *above* watch, a lower one a *below* watch; only when the target equals the current price (or there is none yet) does it ask. The price may be typed in Dutch or English notation (`68.000`, `68.000,00`, `68000.5`, `68,5`; a single `.` followed by exactly three digits groups thousands, so write `2,479` for 2.479) and must be a finite number above 0 (zero or negative is allowed for an asset with `allow_negative`, like power). Before a fetched price is checked against the watches it must be plausible: a finite number, above 0 unless the asset has `allow_negative`, and within the asset's `max_jump_pct` of the last plausible price; a bigger jump only counts when the next poll confirms it (a one-poll glitch never fires an alert). Ignored prices are logged. When the target is reached you get a macOS notification and the menu bar icon switches from its monochrome template (which follows the menu bar) to the coloured alert icon. A watch fires once, until you reset it (`ticker reset`) or remove it. Click a watch in the menu to remove it. *Manage Watches* lists them and can clear all. Watch prices, notifications, *Manage Watches* and the CLI show the asset's own unit (`€`, `$`, `£`, `¥`, or the code, e.g. `XAU 1,00`).
- **Copy to clipboard**: copies all current prices as TSV (symbol, name, price, unit, unit_hint, day_open, change_day, pct_day, direction_day).
- **Edit asset…**: change an asset's URL (`https://` only), currency/unit and JSON price path through dialogs. Edits are saved to the user config. Responses over 1 MB are refused.
- **Reset assets to defaults**: deletes the user config and goes back to the bundled `config.toml`.
- Prices use Dutch number formatting (`1.234,56`). The menu bar value is rounded for prices ≥ 100.

## Configuration

Ticker reads `~/.ticker_config.toml` if it exists. Otherwise it falls back to the bundled [`config.toml`](config.toml): `Contents/Resources/config.toml` inside the app, or `apps/ticker/config.toml` during `cargo run`. Set `TICKER_USER_CONFIG_PATH` to use a different user config path.

```toml
# Asset shown in the menu bar; falls back to the first asset with a valid price.
menubar_asset = "Bitcoin"

[[assets]]
name = "Bitcoin"
url = "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=eur"
price_path = "bitcoin.eur"
unit = "EUR"          # EUR, USD, GBP, JPY show as €, $, £, ¥; any other code is shown in front (`XAU 1,00`)
unit_hint = "/BTC"
symbol = "💰"
```

| Field | Meaning |
| --- | --- |
| `name` | Label, and the key for watches and the menu bar choice |
| `url` | HTTP GET endpoint that returns JSON |
| `price_path` | Dot path into the JSON. Numeric parts index arrays, `field=value` picks the array element whose `field` equals `value` (e.g. `countries.countryCode=NL.petrolPrice`), and `@now` picks, from an array of entries with an RFC 3339 `time`, the one whose period contains the current time (e.g. `data.@now.price` for the hourly or quarter-hourly day-ahead power price). An entry's period runs until the next entry's `time`; the last one lasts as long as the step before it. The value may be a number or a numeric string, and may be negative. |
| `unit` | Currency code |
| `unit_hint` | Suffix after the price, e.g. `/MWh` |
| `symbol` | Emoji/prefix in the menu |
| `allow_negative` | Optional, default `false`. Prices (and watch targets) at or below 0 are real for this asset; otherwise such a price never sets off a watch. The bundled Power NL sets `true` |
| `max_jump_pct` | Optional, default `25`. A move of more than this (in %) from the last plausible price sets off a watch only when the next poll confirms it. The bundled Power NL uses `300` |

All other fields are required per asset. A user config for the `dap.xadi.eu` power price without `allow_negative` / `max_jump_pct` gets the bundled values when it is loaded. `menubar_asset` is optional. A menu bar asset picked by clicking a row takes precedence over it. If the config cannot be loaded, the menu shows the error and *Retry* loads it again (after you fixed the file). An asset with an unusable URL (not `https://`, no host, credentials in it) or an empty price path is skipped instead: it gets a ⚠︎ error line in the menu, the other assets keep working, and *Edit asset…* can fix it.

State files in your home directory (without one, Ticker uses the bundled config, logs to stderr only and keeps the price history and watches in memory):

| File | Contents |
| --- | --- |
| `~/.ticker_config.toml` | User config (written by *Edit asset…*) |
| `~/.ticker_menubar_asset` | Menu bar asset picked from the menu |
| `~/.ticker_watches.json` | Price watches. If it cannot be parsed at start, the menu bar app moves it to `.ticker_watches.json.bak` (`.1.bak`, … if that exists), starts with no watches and shows a notification |
| `~/.ticker_price_history.json` | Day-open and last-poll prices. A file that cannot be parsed is moved to `.ticker_price_history.json.bak` (logged) and the history starts over |
| `~/.ticker_watches.json.lock` | Lock held while the CLI or the menu bar app loads, changes and saves the watches, so neither overwrites the other's change |
| `~/.ticker_debug.log` | Log of the menu bar app, recreated at every start once it holds the instance lock (CLI commands never touch it) |
| `~/.ticker.lock` | Single-instance lock of the menu bar app (next to `.ticker_config.toml`; a second start exits, CLI commands ignore it) |

Config, pin, history and watches are saved atomically: written to a temporary file in the same directory, fsynced, then renamed over the old one, so a crash or full disk never leaves half a file. A symlinked file (for example from a dotfiles repo) stays a symlink; its target is replaced.

## Watch CLI

Run the binary with arguments to manage watches without starting the menu bar app. Commands may run while the app runs: each one loads, changes and saves the watch file under its lock, and the app reloads the file at its next poll when it changed. `clear` loads the file first, so a corrupt file is reported instead of overwritten.

```bash
ticker add Bitcoin 68000 above
ticker add Gold 2000 below
ticker list
ticker remove Bitcoin 68000
ticker clear          # remove all watches
ticker reset          # re-arm triggered watches
ticker help
```

Prices are read like in the app's prompt (`ticker add Bitcoin 68.000 above` is 68000). From the repo: `cargo run -p ticker -- list`. From an installed app: `/Applications/Ticker.app/Contents/MacOS/ticker list`.

## Build and run

From the repo root:

```bash
cargo run -p ticker
cargo test -p ticker
cargo clippy -p ticker --all-targets -- -D warnings
cargo run -p ticker --features dhat-heap   # heap profiling with dhat
```

Local app bundle (needs `cargo-bundle`; `config.toml` is bundled as a resource):

```bash
cd apps/ticker && cargo bundle --release
```

## Release

```bash
scripts/tag_release.sh ticker
```

This bumps the patch version and tags `ticker-vX.Y.Z`. GitHub Actions then builds an ad-hoc signed `ticker-X.Y.Z.dmg` (Apple silicon) and publishes it on [Releases](https://github.com/marcelkoopman/mac-apps/releases). See the [root README](../../README.md#release) for details.

## License

MIT, see [LICENSE](LICENSE).
