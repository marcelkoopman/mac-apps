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

### Bijwerken vanaf Copycraft 0.1.87 of eerder

CopyCraft heeft nu de bundle-ID `nl.marcelkoopman.copycraft` (was `com.github.marcelkoopman.mac-apps.copycraft`), dus macOS ziet het als een nieuwe app:

- Stel je instellingen opnieuw in via Instellingen.
- Zet **Openen bij inloggen** opnieuw aan (Instellingen › Algemeen).
- Sta plakken opnieuw toe: kies Sta toe als macOS erom vraagt, of zet CopyCraft op Altijd toestaan in Systeeminstellingen › Privacy en beveiliging › Plakken vanuit andere apps.
- Verwijder het oude Copycraft-onderdeel onder Systeeminstellingen › Algemeen › Inlogonderdelen.
