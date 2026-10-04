# PasteProbe: pasteboard privacy testen op macOS 26/27

Doel: op een echte Mac zien welke `NSPasteboard`-aanroepen van copycraft de melding
"… is trying to access the pasteboard" geven, wat `accessBehavior` zegt, en of een keuze
in *Privacy & Security › Paste from Other Apps* een nieuwe (ad-hoc) build overleeft.
Achtergrond en bronnen: `FINDINGS.md`.

Bestanden:

- `PasteProbe.swift`: één bestand, AppKit. Pollt `changeCount` zoals copycraft (1 s), en
  logt bij een wijziging elke aanroep vóór en na, met duur: `types`,
  `availableType(from:)`, `pasteboardItems.count`, `NSImage.canInit(with:)`,
  `detectedPatterns(for:)`, `detectedMetadata(for:)`, en dan `string(forType:)` (en PNG/TIFF
  `data(forType:)`). De melding is modaal en blokkeert de aanroep: een regel met
  `⚠️ BLOCKED` wijst precies de aanroep aan die de melding gaf.
- `build_app.sh`: bouwt `build/PasteProbe.app` (bundle id `nl.marcelkoopman.pasteprobe`),
  ad-hoc gesigneerd of met `SIGN_ID=…`, en toont CDHash en designated requirement.

Log: stdout én `~/Library/Logs/PasteProbe.log` (`tail -f` in een tweede Terminal-venster).
Elke regel begint (na de tijd) met het proces-id, `[pid 12345]`: staan er twee pids in één
ronde, dan draaiden er twee probes tegelijk en telt die meting niet.
Gekopieerde tekst wordt niet gelogd, alleen de lengte (tenzij `--show`).

Niet getest op een Mac: het script is op Linux alleen op syntax gecontroleerd
(`swiftc -parse`), niet gecompileerd tegen de macOS SDK. Geeft `swiftc` een typefout in
de detect-aanroepen (Swift-API, macOS 15.4+), meld die dan; de rest staat los daarvan
(`--no-detect`).

## Eerst: is de functie aan?

1. `sw_vers` noteren. Volgens de release notes van macOS 27.2 beta 2 is de user default
   `EnablePasteboardPrivacyDeveloperPreview` verwijderd. Op 26.x / 27.0 / 27.1 kun je hem
   per app aanzetten; op 27.2 beta 2 en later niet meer. Zie je daar zonder vlag geen
   melding, dan staat de functie (nog) niet aan voor iedereen.
2. Vlag per app (alleen ≤ 27.1):
   `defaults write nl.marcelkoopman.pasteprobe EnablePasteboardPrivacyDeveloperPreview -bool yes`
   Na de test weer weghalen met `defaults delete nl.marcelkoopman.pasteprobe EnablePasteboardPrivacyDeveloperPreview`.
   Niet in het globale domein (`-g`) zetten: volgens ontwikkelaars verandert de vlag ook het
   gedrag van menu's (TrustedUIService), in elke app.
3. Terugzetten tussen testrondes:
   `tccutil reset Pasteboard nl.marcelkoopman.pasteprobe` (servicenaam `Pasteboard` is
   gemeld door ontwikkelaars, niet door Apple gedocumenteerd).

## Test 1: vanuit Terminal (geen app-bundel)

```sh
cd ~/…/pasteboard-privacy
swift PasteProbe.swift --once            # één momentopname, dan stoppen
swift PasteProbe.swift                    # pollen; kopieer iets in TextEdit
```

Er is geen bundle id, dus de vlag uit stap 2 geldt niet. Probeer de argument-variant
(`swift PasteProbe.swift -EnablePasteboardPrivacyDeveloperPreview YES`): of AppKit die
oppikt is niet bekend. Zed meldde dat de melding alleen met een `.app`-bundel te
reproduceren was. Verwacht: geen melding, `accessBehavior` blijft `default (0)`. Dit is de
controlegroep, niet het echte gedrag.

## Test 2: als app-bundel (ad-hoc, zoals copycraft nu)

```sh
./build_app.sh
defaults write nl.marcelkoopman.pasteprobe EnablePasteboardPrivacyDeveloperPreview -bool yes
pkill -x PasteProbe                         # vóór elke nieuwe ronde: geen oude probe laten draaien
open build/PasteProbe.app --args --window   # zonder -n: één exemplaar
tail -f ~/Library/Logs/PasteProbe.log
```

Start de probe altijd met `open` **zonder** `-n`, en eerst `pkill -x PasteProbe`: `-n` start
bij elke aanroep een extra exemplaar, en meerdere probes die tegelijk pollen geven meerdere
meldingen en een onleesbaar log. (Zonder `-n` negeert `open` de `--args` als de probe al
draait; daarom eerst `pkill`.)

Kopieer telkens iets in een **andere** app (TextEdit, Safari) en kijk naar het log:

| Stap | Doen | Waar je op let |
|---|---|---|
| 2a | Tekst kopiëren, probe pollt met lezen | Welke regel `BLOCKED` is. Verwacht: alleen `string(forType:)` en `data(forType:)`, niet `types` / `availableType` / `changeCount` / detect. Wat de melding biedt (Allow = één keer, Deny). Daarna `accessBehavior`: verwacht `default` → `ask`. |
| 2b | In de melding *Deny* kiezen | Geeft `string(forType:)` `nil`? Komt de melding bij de volgende kopie terug? |
| 2c | Opnieuw starten met `--no-read` (detect-only pollen) | Geen enkele melding. Noteer wat `types`, `detectedPatterns` (bijv. `probableWebURL`, `number`, `emailAddresses`) en `detectedMetadata.contentType` (Finder-bestand kopiëren, bijv. een PNG) wél vertellen. |
| 2d | ⌃⌥⌘P (globale sneltoets, lezen in de handler) | Melding of niet? Verwacht: wel (niet "paste-related"). |
| 2e | Linksklik op "PB" in de menubalk | Idem, verwacht: melding. |
| 2f | Rechtsklik › *Read now (custom menu action)* | Verwacht: melding. |
| 2g | Rechtsklik › *Paste (paste: action)* | Onbekend: telt een menu-item met de `paste:`-action in een status-item-menu als paste-related? |
| 2h | In het venster klikken, dan ⌘V of *Edit › Paste* | Verwacht: geen melding (hoofdmenu Paste). Vergelijk ⌘V met de menukeuze met de muis. |
| 2i | Knop *Read (custom button)* | Verwacht: melding. |
| 2j | In het tekstveld plakken | Controle: gewoon plakken, nooit een melding. |
| 2k | Vanuit Terminal `swift PasteProbe.swift --write-concealed` | `types` toont `org.nspasteboard.ConcealedType` zonder melding; de probe leest niet. |
| 2l | Iets in de probe zelf kopiëren (tekstveld, ⌘C) | Eigen inhoud lezen: verwacht geen melding. |
| 2m | `--detect-values` meenemen | Apple documenteert dat `detectedValues` bij een match leest en dus een melding kan geven: klopt dat? |

Herhaal 2a–2f met `AGENT=1 ./build_app.sh` (LSUIElement, zoals copycraft: geen Dock-icoon,
geen zichtbaar hoofdmenu), `pkill -x PasteProbe` en `open build/PasteProbe.app` (zonder
`--window`).

## Test 3: overleeft "Allow" een nieuwe build?

1. *Rechtsklik › Open Privacy & Security › Paste from Other Apps* (of
   `open "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Pasteboard"`),
   PasteProbe op **Allow** zetten. Probe herstarten: `accessBehavior` = `alwaysAllow (2)`,
   lezen geeft geen melding.
2. `./build_app.sh` opnieuw (ad-hoc: nieuwe CDHash, zie de uitvoer), `pkill -x PasteProbe`,
   `open build/PasteProbe.app`.
   Verwacht (TN3127: een ad-hoc designated requirement hoort bij die ene build): de
   toestemming geldt niet meer, terug naar `default`/`ask`, of Settings zegt "Allow" terwijl
   er wel een melding komt.
3. Hetzelfde met een vaste identiteit: maak in *Keychain Access › Certificate Assistant* een
   self-signed "Code Signing"-certificaat "PasteProbe Test" (of gebruik Developer ID) en bouw
   twee keer met `SIGN_ID="PasteProbe Test" ./build_app.sh`. Verwacht: Allow blijft staan.
4. Na afloop: `tccutil reset Pasteboard nl.marcelkoopman.pasteprobe` en de `defaults delete` uit stap 2.

## Wat je terugmeldt

Per stap: macOS-versie (`sw_vers`), vlag aan/uit, ad-hoc of vaste identiteit, LSUIElement
ja/nee, welke regel `BLOCKED` was, de tekst en knoppen van de melding, `accessBehavior`
vóór en na. Het logbestand is genoeg.
