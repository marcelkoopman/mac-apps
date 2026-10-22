<!--
Shown at the top of the next copycraft release's notes, above GitHub's generated list
(scripts/release_notes.sh, called by .github/workflows/release-copycraft.yml). It ships only
in the first release after this file changed; to add a note for a later release, replace the
text below. Everything inside this comment is left out.
-->
### The app is now called CopyCraft

The app is now named **CopyCraft** (capital C in Craft) in the menu bar, About, Settings, Finder and the Dock, and the app in the disk image is `CopyCraft.app`. The bundle ID stays `nl.marcelkoopman.copycraft`, so your settings are kept.

- Delete the old `Copycraft.app` from Applications after copying `CopyCraft.app` there (Finder may keep both, because the names differ).
- If CopyCraft does not open at login any more, turn **Open at Login** on again (Settings › General) and remove any old Copycraft entry under System Settings › General › Login Items.

### Remove one copy from history

Each small card in the history strip now has a ✕ (on the card shown and the one under the pointer). It removes only that copy from CopyCraft's history; the clipboard keeps what it holds. ⌫ removes the copy on screen, and ⌘Z puts it back. The top button is now called **Clear all** and still asks first.

### Blur is now off by default

Blur is now off by default. Masked content is hidden under a plain cover in your card theme's colours. To get the blur back, tick **Blur masked content** in Settings › Privacy. If you turned the blur on or off yourself before, that choice is kept.

### The privacy filter is now off by default

The privacy filter is now off by default, so copied content shows on the card straight away, without a click to reveal it. To hide it again until you click, tick **Use privacy filter** in Settings › Privacy. If you turned the filter on or off yourself before, that choice is kept. Sensitive copies such as tokens, keys and passwords in links are still kept out of history, whatever this setting says. The **Possibly sensitive text copied** notification also no longer depends on the privacy filter: it follows its own **Sensitive copy** switch in Settings › Notifications, which is on by default.

### Upgrading from Copycraft 0.1.87 or earlier

CopyCraft now has the bundle ID `nl.marcelkoopman.copycraft` (it was `com.github.marcelkoopman.mac-apps.copycraft`), so macOS treats it as a new app:

- Set your settings in Settings again.
- Turn on **Open at Login** again (Settings › General).
- Allow pasting again: choose Allow when macOS asks, or set CopyCraft to Always Allow in System Settings › Privacy & Security › Paste from Other Apps.
- Remove the old Copycraft entry under System Settings › General › Login Items.

### De app heet nu CopyCraft

De app heet nu **CopyCraft** (met een hoofdletter C in Craft) in de menubalk, Over, Instellingen, de Finder en het Dock, en de app in de schijfkopie is `CopyCraft.app`. De bundle-ID blijft `nl.marcelkoopman.copycraft`, dus je instellingen blijven bewaard.

- Verwijder de oude `Copycraft.app` uit Programma's nadat je `CopyCraft.app` daarheen hebt gekopieerd (de Finder kan ze allebei houden, omdat de namen verschillen).
- Opent CopyCraft niet meer bij het inloggen, zet **Openen bij inloggen** dan opnieuw aan (Instellingen › Algemeen) en verwijder een oud Copycraft-onderdeel onder Systeeminstellingen › Algemeen › Inlogonderdelen.

### Eén kopie uit de geschiedenis halen

Elk kaartje in de geschiedenisstrook heeft nu een ✕ (op het getoonde kaartje en het kaartje onder de aanwijzer). Die haalt alleen die kopie uit de geschiedenis van CopyCraft; het klembord houdt wat erop staat. ⌫ haalt de getoonde kopie weg en ⌘Z zet hem terug. De knop bovenaan heet nu **Alles wissen** en vraagt nog steeds eerst om bevestiging.

### Vervagen staat nu standaard uit

Vervagen staat nu standaard uit. Gemaskeerde inhoud zit onder een effen afdekking in de kleuren van je kaartthema. Wil je het vervagen terug, vink dan **Gemaskeerde inhoud vervagen** aan in Instellingen › Privacy. Heb je het vervagen eerder zelf aan- of uitgezet, dan blijft die keuze bewaard.

### Het privacyfilter staat nu standaard uit

Het privacyfilter staat nu standaard uit, dus gekopieerde inhoud staat meteen op de kaart, zonder klik om hem te tonen. Wil je hem weer verborgen houden tot je klikt, vink dan **Gebruik privacyfilter** aan in Instellingen › Privacy. Heb je het filter eerder zelf aan- of uitgezet, dan blijft die keuze bewaard. Gevoelige kopieën zoals tokens, sleutels en wachtwoorden in links komen nog steeds niet in de geschiedenis, wat deze instelling ook zegt. Ook de melding **Mogelijk gevoelige tekst gekopieerd** hangt niet meer af van het privacyfilter: die volgt zijn eigen schakelaar **Gevoelige kopie** in Instellingen › Meldingen, die standaard aan staat.

### Bijwerken vanaf Copycraft 0.1.87 of eerder

CopyCraft heeft nu de bundle-ID `nl.marcelkoopman.copycraft` (was `com.github.marcelkoopman.mac-apps.copycraft`), dus macOS ziet het als een nieuwe app:

- Stel je instellingen opnieuw in via Instellingen.
- Zet **Openen bij inloggen** opnieuw aan (Instellingen › Algemeen).
- Sta plakken opnieuw toe: kies Sta toe als macOS erom vraagt, of zet CopyCraft op Altijd toestaan in Systeeminstellingen › Privacy en beveiliging › Plakken vanuit andere apps.
- Verwijder het oude Copycraft-onderdeel onder Systeeminstellingen › Algemeen › Inlogonderdelen.
