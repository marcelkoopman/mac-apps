# Ticker

A menu bar price ticker for macOS. It shows one asset's price in the menu bar and lists every configured asset in the menu. The default assets are Bitcoin, ETH, gold, TTF gas, NL petrol/diesel and NL power. The app has no Dock icon.

Part of the [mac-apps](../../README.md) monorepo.

## Features

- **Polling**: prices refresh every 5 minutes, or on demand with *Poll now*. Each fetch is tried up to 3 times, and until a fetch succeeds the menu keeps the last known price.
- **Day change**: each menu row shows the price with its unit and the change since the day's first price, e.g. `▲ €217,00 · +0.33%`. That first price is stored per local calendar day.
- **Menu bar asset**: click a price row to show that asset in the menu bar. The choice is remembered.
- **Price watches**: *Add Price Watch* takes an asset, a target price and *above* / *below*. When the target is reached you get a macOS notification and the menu bar icon switches to the alert icon. A watch fires once, until you reset it (`ticker reset`) or remove it. Click a watch in the menu to remove it. *Manage Watches* lists them and can clear all. Watch prices are always shown in €.
- **Copy to clipboard**: copies all current prices as TSV (symbol, name, price, unit, unit_hint, day_open, change_day, pct_day, direction_day).
- **Edit asset…**: change an asset's URL, currency/unit and JSON price path through dialogs. Edits are saved to the user config.
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
unit = "EUR"          # EUR, USD, GBP, JPY show as €, $, £, ¥; anything else is shown as-is
unit_hint = "/BTC"
symbol = "💰"
```

| Field | Meaning |
| --- | --- |
| `name` | Label, and the key for watches and the menu bar choice |
| `url` | HTTP GET endpoint that returns JSON |
| `price_path` | Dot path into the JSON. Numeric parts index arrays, and `field=value` picks the array element whose `field` equals `value` (e.g. `countries.countryCode=NL.petrolPrice`). The value may be a number or a numeric string. |
| `unit` | Currency code |
| `unit_hint` | Suffix after the price, e.g. `/MWh` |
| `symbol` | Emoji/prefix in the menu |

All fields are required per asset. `menubar_asset` is optional. A menu bar asset picked by clicking a row takes precedence over it.

State files in your home directory:

| File | Contents |
| --- | --- |
| `~/.ticker_config.toml` | User config (written by *Edit asset…*) |
| `~/.ticker_menubar_asset` | Menu bar asset picked from the menu |
| `~/.ticker_watches.json` | Price watches |
| `~/.ticker_price_history.json` | Day-open and last-poll prices |
| `~/.ticker_debug.log` | Log, recreated at every start |
| `~/.ticker.lock` | Single-instance lock of the menu bar app (next to `.ticker_config.toml`; a second start exits, CLI commands ignore it) |

## Watch CLI

Run the binary with arguments to manage watches without starting the menu bar app:

```bash
ticker add Bitcoin 68000 above
ticker add Gold 2000 below
ticker list
ticker remove Bitcoin 68000
ticker clear          # remove all watches
ticker reset          # re-arm triggered watches
ticker help
```

From the repo: `cargo run -p ticker -- list`. From an installed app: `/Applications/Ticker.app/Contents/MacOS/ticker list`.

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
