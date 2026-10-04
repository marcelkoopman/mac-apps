# Pasteboard privacy op macOS: wat het betekent voor copycraft (stand 4 okt 2026)

Labels: **BEVESTIGD** = staat in Apple-documentatie of release notes. **GEMELD** = waargenomen
door ontwikkelaars (blog, issue), niet door Apple. **VERWACHT** = mijn afleiding. **GEMETEN** =
zelf gemeten met PasteProbe.

## 1. Status, accessBehavior, welke aanroepen

- **BEVESTIGD** Aangekondigd in april 2025 (AppKit updates, "macOS pasteboard privacy") als
  *upcoming feature*, met de API's vanaf **macOS 15.4** en een opt-in testvlag
  `defaults write <bundle id> EnablePasteboardPrivacyDeveloperPreview -bool yes`.
  https://developer.apple.com/documentation/updates/appkit#macOS-pasteboard-privacy
- **BEVESTIGD** Geen vermelding in de AppKit updates van juni 2025 / juni 2026, en niet in de
  release notes van macOS 26 t/m 26.6 en 27.0. Wel in de **macOS 27.2 beta 2** release notes
  (AppKit › Deprecations): "The `EnablePasteboardPrivacyDeveloperPreview` user default has been
  removed. (186955507)".
  https://developer.apple.com/documentation/macos-release-notes/macos-27_2-release-notes
- **GEMELD** In macOS 26 staat het zonder vlag niet aan (Felix Schwarz, sept 2025). Op 27.2 beta
  zag Léo Natan geen meldingen en geen instellingenpaneel (sept 2026).
  https://mjtsai.com/blog/2025/05/12/pasteboard-privacy-preview-in-macos-15-4/
- Er is **geen bron** voor een fase waarin macOS 26 alleen logt of waarschuwt. Apple heeft niet
  gezegd of het weghalen van de vlag betekent dat de functie vervalt of straks voor iedereen aan
  gaat. Ook geen WWDC25/26-sessie gevonden.
- **GEMETEN** (macOS 27.0.1, PasteProbe, 4 okt 2026) Zonder de vlag is `accessBehavior` =
  `alwaysAllow`; met `EnablePasteboardPrivacyDeveloperPreview` is het `default` bij de start. Of
  "Allow" in de melding onthouden wordt is niet bevestigd (er draaiden meerdere probes tegelijk,
  door `open -n` in de README); de Deny-test staat nog open.
- **BEVESTIGD** `NSPasteboard.accessBehavior` (alleen-lezen, macOS 15.4+), enum
  `NSPasteboard.AccessBehavior`: `default` (0), `ask` (1), `alwaysAllow` (2), `alwaysDeny` (3).
  `default` betekent: voor het general pasteboard bij programmatische toegang vragen (andere
  pasteboards: altijd toestaan). Een app die nooit een melding gaf meldt `.default` en staat niet
  in Systeeminstellingen. Na de eerste melding wordt het `.ask` en kan de gebruiker per app
  kiezen tussen ask/alwaysAllow/alwaysDeny. Bij `ask` en `alwaysDeny`: "access that is both
  user originated and paste related will always be allowed".
  https://developer.apple.com/documentation/appkit/nspasteboard/accessbehavior-swift.enum
  (dezelfde waarden staan in objc2-app-kit 0.3.2 `NSPasteboardAccessBehavior`)
- **GEMELD** Waar het staat: *Systeeminstellingen › Privacy & Security › Paste from Other Apps*,
  met Ask / Allow / Deny. URL macOS 26:
  `x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Pasteboard`.
  Resetten: `tccutil reset Pasteboard <bundle id>`. In de melding zelf betekent "Allow" één
  keer; alleen in Settings is het blijvend.
- **BEVESTIGD (alleen ruw)** Welke aanroepen: Apple zegt "programmatically reads the general
  pasteboard". De `detect`-methoden laten zien wat voor soort data er staat "without actually
  reading them". Het gedrag is "similar to how UIPasteboard behaves in iOS". Voor iOS staat er
  een lijst met wat géén melding geeft: `numberOfItems`, `types`, `hasStrings`/`hasURLs`/…,
  `canLoadObject`, en de detect-methoden.
  https://developer.apple.com/documentation/uikit/uipasteboard
  Voor macOS staat er per methode niets bij `changeCount`, `types`, `availableType(from:)`,
  `string(forType:)`, `data(forType:)` of `readObjects`.
- **GEMELD** In de stacktrace van Zed komt de melding uit `CFPasteboardCopyData` →
  `requestDataForPasteboard` → `NSAlert runModal`, aangeroepen vanuit `-[NSPasteboard
  _dataForType:…]`. De melding is dus modaal en blokkeert de aanroepende thread.
  https://github.com/zed-industries/zed/issues/30752
- **VERWACHT** `changeCount`, `types`, `availableTypeFromArray:` en `pasteboardItems` (zonder
  data) geven geen melding. `stringForType:`, `dataForType:`, `propertyListForType:`,
  `readObjectsForClasses:` en `NSImage initWithPasteboard:` wel, tenzij de inhoud van de app
  zelf komt. De test (stap 2a) moet dit bevestigen.

## 2. Detect-API's

- **BEVESTIGD** macOS 15.4+, op `NSPasteboard` (eerste item) en `NSPasteboardItem`.
  - Swift: `detectedPatterns(for:)`, `detectedValues(for:)`, `detectedMetadata(for:)`
    (async throws, key paths van `NSPasteboard.DetectedValues` / `NSPasteboard.DetectedMetadata`).
  - ObjC/objc2: `detectPatternsForPatterns:completionHandler:`,
    `detectValuesForPatterns:completionHandler:`, `detectMetadataForTypes:completionHandler:`.
  - Let op: `detectPatterns(for:)` en `DetectionPattern` zijn de iOS-namen (UIPasteboard).
    Op macOS heet het type in ObjC `NSPasteboardDetectionPattern`.
  https://developer.apple.com/documentation/appkit/nspasteboard/detectedpatterns(for:)
- **BEVESTIGD** Patronen: ProbableWebURL, ProbableWebSearch, Number, Link, PhoneNumber,
  EmailAddress, PostalAddress, CalendarEvent, ShipmentTrackingNumber, FlightNumber, MoneyAmount.
  Metadata: alleen ContentType (de UTType van een bestands-URL op het pasteboard).
  https://developer.apple.com/documentation/appkit/nspasteboard/detectedvalues
- **BEVESTIGD** `detectPatterns` en `detectMetadata` geven geen melding. `detectValues` leest de
  inhoud bij een match en kan dan een melding geven; bij Deny volgt een error.
- **BEVESTIGD (lijst)** Er is geen patroon voor JSON, CSV, code, base64, credentials, enz. De
  formaatherkenning (`format::detect`) en de gevoeligheidslabels van copycraft hebben dus altijd
  de tekst zelf nodig.
- **VERWACHT** Zonder melding kan copycraft wel weten: tekst, afbeelding of bestand (`types`),
  Concealed/Transient/AutoGenerated (`types`), URL/getal/e-mail/telefoon (detect), en het
  bestandstype van een gekopieerd Finder-bestand (`contentType`). Genoeg voor het icoon, de
  tooltip en "Hidden content". Niet genoeg voor de kaart.
- **BEVESTIGD (bron)** objc2-app-kit 0.3.2 heeft `accessBehavior()`,
  `detectPatternsForPatterns_completionHandler`, `detectMetadataForTypes_completionHandler` en
  de `NSPasteboardDetectionPattern*`-constanten. De detect-methoden vragen de `block2`-feature,
  die in `objc2-app-kit/default` zit (`appkit-full`).

## 3. Telt de sneltoets of een klik op het menubalk-icoon?

- **BEVESTIGD** Zonder melding mag alleen toegang die "user originated **and** paste related" is,
  oftewel "input on a UI element that the system considers paste-related". Apple publiceert geen
  lijst voor macOS. Voor iOS noemt Apple: systeemknoppen (UIPasteControl) en Command-V. Een
  AppKit-tegenhanger van UIPasteControl bestaat niet (geen `NSPasteControl` in de docs).
- **GEMELD** Uit Zed (Ask): *Edit › Paste* in het hoofdmenu werkt zonder melding, en Paste in
  het contextmenu ook. ⌘V via eigen toetsafhandeling gaf wel een melding. Inhoud die de app
  zelf had gekopieerd gaf nooit een melding. Volgens Gui Rambo verplaatst de vlag het hoofdmenu
  naar een systeemproces (TrustedUIService), en zo herkent het systeem "Paste". Een ontwikkelaar
  zag dat eigen Dock-menu- en File-menu-acties allebei een melding gaven.
- **VERWACHT** De globale sneltoets van copycraft (Carbon `RegisterEventHotKey` via
  global-hotkey) en de klik op het status item (tray-icon) zijn door de gebruiker gestart, maar
  niet paste-related. Ze geven dus een melding bij `ask` en worden stil geweigerd bij
  `alwaysDeny`. Copycraft leest bovendien pas later in `about_to_wait`, niet in de
  event-handler zelf. Onbekend: of een menu-item met de `paste:`-action in een status-item-menu,
  of ⌘V in het (key) kaartpaneel via een hoofdmenu-item Edit › Paste, meetelt (test 2g/2h).

## 4. Gevolgen

- **VERWACHT** Achtergrondgeschiedenis werkt alleen met `alwaysAllow`, want de inhoud moet
  gelezen worden voordat de volgende kopie hem vervangt. Bij `ask`: bij elke nieuwe kopie een
  modale melding op de main thread, en "Allow" geldt één keer. Bij Deny geeft de leesactie `nil`
  terug. Copycraft ziet dan "NoText", zet `poll_again` en kijkt elke tick opnieuw: zie het
  risico hieronder.
- **VERWACHT** ConcealedType blijft werken: `read_marks` gebruikt alleen `types()` en draait al
  vóór `read_view`. Dat is de goede volgorde.
- **VERWACHT** De wis na 60 s (`clear_sensitive_when_due`: `changeCount` +
  `clearContents`) is een schrijfactie, dus geen melding. Wissen bij vergrendelen, slaap en
  bewaartijd raakt het pasteboard niet. `clear_secrets` en `labels_checked` lezen alleen
  copycrafts eigen inhoud.
- **BEVESTIGD (algemeen, TN3127)** Ad-hoc code heeft een designated requirement "tied to that
  specific version of the code". Unsigned code heeft er geen, dus macOS kan de app niet
  betrouwbaar volgen bij privacy-toestemmingen. MAS- en Developer ID-builds met compatibele
  DR's delen toestemmingen.
  https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements
  **VERWACHT** Dit geldt ook voor de TCC-dienst Pasteboard: bij ad-hoc moet de gebruiker na
  elke release opnieuw "Allow" kiezen, of Settings toont nog Allow terwijl het niet meer werkt.
  Met Developer ID (team-ID + bundle id) blijft de keuze staan (test 3).
- **VERWACHT** Sandbox of App Store maakt voor deze melding niets uit: Apple noemt geen
  onderscheid. **GEMELD** ClipBar kwam door App Review met een onboarding-melding die naar de
  instelling verwijst. Apple (antwoord op FB17587626) wil wel onboarding naar Settings, maar geen
  API om "always allow" aan te vragen. Die API bestaat niet; de FB staat nog open. Een MDM-sleutel
  ontbrak in 26 beta 4 (FB19085787).
  https://github.com/feedback-assistant/reports/issues/655

## Copycraft: aanroepen ↔ bevindingen

Polling (`menubar.rs` `about_to_wait` → `note_clipboard`, r. 2589):

| Aanroep | Waar | Wanneer | Melding? |
|---|---|---|---|
| `changeCount` | `macos_pasteboard.rs:92` | elke tick (1 s / 400 ms) | VERWACHT nee |
| `types()` (marks) | `:259` `read_marks` | bij een nieuwe changeCount | VERWACHT nee |
| `stringForType(String)` | `:712/870` `read_view` | bij een nieuwe changeCount, niet-privé | **ja** |
| `availableTypeFromArray` (FileURL, PNG/TIFF/JPEG/HEIC) | `:784/895` | idem | VERWACHT nee |
| `stringForType(FileURL)`, `propertyListForType(NSFilenamesPboardType)` | `:899/907` `image_file` | idem, en **elke tick** zolang `poll_again` (Empty/NoText) via `pasteboard_has_image` (`:61/773`) | **ja**: risico op herhaalde meldingen |
| `NSImage canInitWithPasteboard` | `:731/775` | idem | onbekend, VERWACHT nee |
| `dataForType` PNG/JPEG/TIFF/HEIC/…, `readObjectsForClasses`, `NSImage initWithPasteboard` | `:155` `current_image_bytes`, `macos_preview_image.rs:17/29` | `record_current` bij een afbeelding | **ja** |
| `dataForType` (decode_preview) | `:175`, op een **achtergrondthread** (`ensure_image_scan`) | kaart open met een afbeelding | **ja**, en een modale melding vanaf een achtergrondthread is onvoorspelbaar |

Gebruikersactie: sneltoets ⌃⌥⌘C (`summon_popup`) en linksklik op het icoon
(`toggle_popup_under_icon`) roepen allebei `launch_for_popup` → `from_os` aan. Meestal komt dat
uit de cache, omdat de poller al gelezen heeft. Dit is niet paste-related, dus VERWACHT een
melding bij `ask`. Schrijven (`write_text`, `write_history_image`, `add_concealed`,
`clear_clipboard`) VERWACHT geen melding.

## Opties

- **A. Pollen zoals nu, plus onboarding naar "Allow".** Bij de start en na elke leesactie
  `accessBehavior` loggen en checken. Bij `default`/`ask`/`alwaysDeny`: een kaart met uitleg en
  de weg naar Settings. Alle functies blijven. Nadelen: tot de gebruiker "Allow" kiest, komt er
  bij elke kopie een modale melding. Bij ad-hoc is de keuze na elke release waarschijnlijk weg.
- **B. Alleen detecteren tijdens het pollen, lezen bij sneltoets of klik.** Geen meldingen op de
  achtergrond, maar ook geen tekstgeschiedenis (alleen wat je opende). Lezen bij sneltoets of
  klik geeft bij `ask` toch elke keer een melding, en bij `alwaysDeny` niets. Het lost het
  probleem dus maar half op.
- **C. Hybride (aanbevolen).** Bij `alwaysAllow`: het huidige gedrag. Anders: alleen
  `changeCount` + `types` + `detectPatterns`/`detectMetadata` (icoon, tooltip, Hidden),
  geen lezen op de achtergrond, en lezen alleen als de kaart geopend wordt (één melding per
  opening bij `ask`). De kaart toont dan eenmalig "Geschiedenis vraagt *Paste from Other Apps ›
  Allow*". Bij `alwaysDeny` geen leespogingen, wel uitleg.

## Aanbeveling

C, maar nog niets inbouwen zolang Apple niets aanzet. Er is nu niets afgedwongen, en de vlag is
in 27.2 beta 2 weg zonder aankondiging dat de functie komt.

Nu al goedkoop en nuttig:

1. `accessBehavior` loggen. Dat zit al in objc2-app-kit 0.3.2.
2. De `poll_again`-lus aanpassen, zodat lezen bij Empty/NoText niet elke tick opnieuw gebeurt
   (alleen opnieuw als `types` verandert), en `nil` na een Deny niet als "opnieuw proberen"
   behandelen.
3. Geen pasteboard-reads op een achtergrondthread (`scan_card_image`): eerst de bytes op de main
   thread kopiëren.
4. Overstappen op een vaste signing-identiteit (Developer ID). Een self-signed certificaat kan
   ook, maar dan blijft Gatekeeper lastig. Anders moeten gebruikers na elke release opnieuw
   "Allow" kiezen.

Draai daarna de test (README) op 26.x/27.1 met de vlag, om de VERWACHT-punten te
bevestigen of te verwerpen.
