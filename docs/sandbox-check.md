# App Sandbox controleren

Beide apps worden ad-hoc gesigned met `apps/<app>/assets/entitlements.plist`, in CI (`release-<app>.yml`) en lokaal met `scripts/sign_app.sh <app>`.

## 1. Entitlements in de signature

```bash
codesign -d --entitlements - /Applications/Ticker.app
codesign -d --entitlements - /Applications/Copycraft.app
```

Je hoort `com.apple.security.app-sandbox` = `true` te zien, plus:

- Ticker: `com.apple.security.network.client`
- Copycraft: `com.apple.security.network.client` en `com.apple.security.files.user-selected.read-write`

`codesign -dv <App>.app` toont `flags=0x10002(adhoc,runtime)`: ad-hoc gesigned, met hardened runtime.

## 2. Draait de app echt in de sandbox?

1. Start de app.
2. Open **Activiteitenweergave** (Activity Monitor).
3. Kies **Weergave › Kolommen › Sandbox** (Engels: View › Columns › Sandbox).
4. Zoek `Ticker` of `Copycraft`. In de kolom **Sandbox** hoort **Ja** te staan.

## 3. Waar staan de bestanden nu?

Een app in de sandbox schrijft in zijn eigen container, niet meer direct in je thuismap:

```
~/Library/Containers/com.github.marcelkoopman.ticker/Data/
~/Library/Containers/com.github.marcelkoopman.copycraft/Data/
```

`HOME` wijst binnen de app naar die `Data/`-map. De dotfiles van Ticker (`.ticker_config.toml`, `.ticker_watches.json`, `.ticker_menubar_asset`, `.ticker_price_history.json`, `.ticker_debug.log`) staan dus in `~/Library/Containers/com.github.marcelkoopman.ticker/Data/`. Je oude bestanden in `~` worden niet automatisch meegenomen. Kopieer ze met de app gesloten zelf naar die map als je je instellingen wilt houden.

Draai je `cargo run -p ticker`, dan werk je zonder sandbox en gebruikt de app nog steeds de bestanden in `~`.

Zie je sandbox-meldingen? Kijk in **Console** en filter op `Sandbox` of `deny` terwijl de app draait.
