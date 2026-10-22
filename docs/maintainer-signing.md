# Maintainer notes: signing and notarization

For the maintainer only, not for contributors. Moved out of the README.

## Developer ID-signing en notarisatie (voorbereid, staat uit)

De release-workflows kunnen de app en de DMG met een Developer ID-certificaat signen, laten notariseren door Apple en het ticket stapelen. Dat gebeurt alleen als de secrets hieronder bestaan; zonder die secrets blijft alles zoals het is (ad-hoc signing, geen notarisatie). Een stap `Detect signing secrets` kijkt alleen óf ze er zijn en logt `Signing: ad-hoc|developer-id, notarize: true|false`.

Wat er met de secrets gebeurt (`release-<app>.yml`):

1. `scripts/import_signing_cert.sh` zet de `.p12` in een tijdelijke keychain (willekeurig wachtwoord, `set-key-partition-list` voor codesign, Apple's Developer ID G2-CA erbij als die ontbreekt). Aan het eind van de job wordt die keychain verwijderd.
2. `scripts/sign_app.sh <app> <App.app>` met `MACOS_SIGN_IDENTITY`: `codesign --options runtime --timestamp --entitlements apps/<app>/assets/entitlements.plist` (hardened runtime, App Sandbox, secure timestamp). Het script controleert de `Developer ID Application`-authority, de timestamp, de hardened runtime en `com.apple.security.app-sandbox`.
3. `scripts/notarize.sh <App.app>`: zip met `ditto`, `xcrun notarytool submit --wait`, bij een andere status dan `Accepted` het `notarytool log` en een rode build, daarna `xcrun stapler staple` + `stapler validate` en `spctl --assess`.
4. DMG bouwen met de gestapelde app, de DMG signen (`codesign --timestamp`), `scripts/notarize.sh <dmg>` en stapelen.

Alleen het certificaat (zonder notarisatie-secrets) geeft een Developer ID-gesigneerde maar niet-genotariseerde release, met een waarschuwing in de log. Alleen notarisatie-secrets (zonder certificaat) doen niets: een ad-hoc-signature kan niet genotariseerd worden.

### Secrets (Settings → Secrets and variables → Actions → Repository secrets)

Signing, alle drie nodig:

| Secret | Inhoud |
| --- | --- |
| `MACOS_CERT_P12` | Het Developer ID Application-certificaat mét private key als `.p12`, base64: `base64 -i DeveloperID.p12 \| pbcopy` |
| `MACOS_CERT_PASSWORD` | Het wachtwoord dat je bij het exporteren van de `.p12` koos |
| `MACOS_SIGN_IDENTITY` | De naam van de identiteit, precies zoals `security find-identity -v -p codesigning` hem toont, bijvoorbeeld `Developer ID Application: Marcel Koopman (ABCDE12345)` |

Notarisatie, één van de twee sets (de API-key wint als beide er zijn):

| Secret | Inhoud |
| --- | --- |
| `APPLE_API_KEY_P8` | App Store Connect API-key: de inhoud van `AuthKey_<KEYID>.p8` (de tekst, of die tekst base64) |
| `APPLE_API_KEY_ID` | De Key ID van die key (10 tekens) |
| `APPLE_API_ISSUER_ID` | De Issuer ID (UUID) bovenaan de lijst met keys |
| *of* `APPLE_ID` | Het e-mailadres van je Apple-account |
| `APPLE_TEAM_ID` | Je Team ID (10 tekens, developer.apple.com → Account → Membership details) |
| `APPLE_APP_PASSWORD` | Een app-specifiek wachtwoord voor dat account |

### Zo maak je ze

1. **Certificaat** (betaald Apple Developer Program nodig). Op developer.apple.com → Certificates → `+` → *Developer ID Application* (profiel G2 Sub-CA), met een CSR uit Sleutelhangertoegang (*Certificaatassistent → Vraag certificaat aan bij certificaatautoriteit*, opslaan op schijf). Download het `.cer` en dubbelklik het, zodat het bij je private key in de login-keychain komt.
2. **`.p12` exporteren**: in Sleutelhangertoegang onder *Mijn certificaten* het certificaat *Developer ID Application: …* (met de sleutel eronder) selecteren → *Exporteer* → `.p12`, met een sterk wachtwoord. Dan `base64 -i DeveloperID.p12 | pbcopy` en plakken als `MACOS_CERT_P12`; het wachtwoord wordt `MACOS_CERT_PASSWORD`. Gooi de `.p12` daarna weg of berg hem veilig op.
3. **Identiteit**: `security find-identity -v -p codesigning` en de tekst tussen de aanhalingstekens kopiëren naar `MACOS_SIGN_IDENTITY`.
4. **API-key** (aanbevolen): App Store Connect → Gebruikers en toegang → Integraties → *App Store Connect API* → Team Keys → `+`, rol *Developer*. Download `AuthKey_<KEYID>.p8` (kan maar één keer). Zet de inhoud (`pbcopy < AuthKey_<KEYID>.p8`) in `APPLE_API_KEY_P8`, de Key ID in `APPLE_API_KEY_ID` en de Issuer ID in `APPLE_API_ISSUER_ID`.
5. **Of Apple ID**: op account.apple.com → Inloggen en beveiliging → *App-specifieke wachtwoorden* een wachtwoord maken (`APPLE_APP_PASSWORD`), plus `APPLE_ID` en `APPLE_TEAM_ID`.

Met de GitHub CLI kan het ook: `gh secret set MACOS_CERT_P12 < <(base64 -i DeveloperID.p12)`, `gh secret set APPLE_API_KEY_P8 < AuthKey_<KEYID>.p8`, enzovoort.

Lokaal werkt hetzelfde zonder tijdelijke keychain (het certificaat staat in je login-keychain):

```bash
MACOS_SIGN_IDENTITY="Developer ID Application: … (TEAMID)" scripts/sign_app.sh copycraft
APPLE_API_KEY_P8="$(cat AuthKey_XXXX.p8)" APPLE_API_KEY_ID=XXXX APPLE_API_ISSUER_ID=… \
  scripts/notarize.sh target/aarch64-apple-darwin/release/bundle/osx/CopyCraft.app
```

Pas na de eerste release-tag mét secrets is dit echt getest: CI op `main` draait de release-workflows niet.

## Mac App Store-pakket (voorbereid, nog nooit gedraaid)

Los van de Developer ID-route hierboven en niet in CI: `scripts/build_store_pkg.sh` maakt lokaal een ondertekende `.pkg` van CopyCraft voor upload met Transporter.

1. Maak in het developer-portaal een App ID `nl.marcelkoopman.copycraft`, een *Apple Distribution*- en een *Mac Installer Distribution*-certificaat (met private key in de login-keychain) en een *Mac App Store*-provisioning profile voor die App ID.
2. Zet de omgeving en draai het script:

   ```bash
   export APPLE_TEAM_ID=ABCDE12345
   export STORE_APP_IDENTITY="Apple Distribution: Naam (ABCDE12345)"
   export STORE_INSTALLER_IDENTITY="3rd Party Mac Developer Installer: Naam (ABCDE12345)"
   export STORE_PROVISION_PROFILE=~/Downloads/Copycraft_MAS.provisionprofile
   scripts/build_store_pkg.sh            # → target/store/CopyCraft.pkg
   ```

Het script stopt met een duidelijke melding als een identiteit niet in de keychain staat (`security find-identity`), het profiel ontbreekt of voor een ander team of een andere app is. Het vult `TEAMID` in `apps/copycraft/assets/entitlements.store.plist` in een tijdelijke kopie in; het bestand in de repo houdt de placeholder.
