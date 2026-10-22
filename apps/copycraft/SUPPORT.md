# CopyCraft support / ondersteuning

Draft texts for the support page and the App Store listing. Check the setting names against the app before publishing.

## Nederlands

### Support

Heb je een vraag of een bug gevonden? Mail naar mac-apps-support@proton.me. Bugs los ik zo goed mogelijk op, maar zonder vaste reactietijd, zonder garantie en zonder terugbetaling door mij. Terugbetalingen lopen via Apple.

### Veelgestelde vragen

1. **Hoe open ik CopyCraft?** Met de sneltoets (standaard ⌃⌘C) of via het icoon in de menubalk. De sneltoets verander je in Instellingen.
2. **Waarom is mijn tekst verborgen of vaag?** Dat is het privacyfilter. Het staat standaard uit; alleen als je "Gebruik privacyfilter" aanzet in Instellingen › Privacy blijft gekopieerde inhoud verborgen tot je erop klikt. Klik op de tekst om hem te tonen. Wil je het weer uit, zet dan "Gebruik privacyfilter" uit. Verborgen inhoud krijgt standaard een effen vlak; zet "Gemaskeerde inhoud vervagen" aan voor vervaging. Gevoelige tekst zoals een token komt nooit in de geschiedenis, ook met het filter uit.
3. **Moet ik iets toestaan bij het klembord?** Op nieuwere macOS-versies vraagt het systeem of CopyCraft het klembord van andere apps mag lezen. Kies Sta toe, of stel het in via Privacy en beveiliging, Plakken vanuit andere apps.
4. **Gaat mijn data het internet op?** Nee. CopyCraft werkt volledig offline en heeft geen netwerktoegang.
5. **Hoe wis ik wat ik gekopieerd heb?** Gebruik Wis of Geschiedenis wissen in de kaart of in het menu.
6. **Hoe maak ik een schermafbeelding van de kaart?** Standaard blokkeert CopyCraft dat. Kies in het menu "Schermafbeeldingen toestaan" om het voor deze sessie toe te staan.
7. **Een app moet genegeerd worden.** Voeg die toe bij Genegeerde apps in Instellingen.
8. **Hoe start ik CopyCraft bij inloggen?** Zet "Openen bij inloggen" aan in Instellingen.

9. **Hoe stel ik een feature voor of meld ik een probleem?** Kies "Feature voorstellen…" of "Probleem melden…" in het ⋯-menu of in Instellingen. Vul een onderwerp en je tekst in en kies "Maak e-mail": je mailapp opent met een concept naar mac-apps-support@proton.me. Jij verstuurt de e-mail zelf; CopyCraft verstuurt niets. De mail bevat alleen je eigen tekst plus de versie van CopyCraft en macOS, nooit je klembord, geschiedenis, bestandsnamen of instellingen. Is je tekst te lang voor een e-maillink, of heb je geen mailapp, dan kun je de tekst naar het klembord kopiëren en zelf in een mail plakken.

## English

### Support

Question or found a bug? Email mac-apps-support@proton.me. I fix bugs as best I can, but with no fixed response time, no warranty and no refunds from me. Refunds go through Apple.

### FAQ

1. **How do I open CopyCraft?** With the hotkey (default ⌃⌘C) or from the menu bar icon. Change the hotkey in Settings.
2. **Why is my text hidden or blurred?** That is the privacy filter. It is off by default; only when you tick "Use privacy filter" in Settings › Privacy does copied content stay hidden until you click it. Click the text to reveal it. To turn it off again, untick "Use privacy filter". Hidden content shows as a plain cover by default; tick "Blur masked content" to see a blur instead. Sensitive text such as a token is never kept in history, also with the filter off.
3. **Do I have to allow clipboard access?** On newer macOS versions the system asks whether CopyCraft may read the clipboard from other apps. Choose Allow, or set it in Privacy & Security, Paste from Other Apps.
4. **Does my data leave my Mac?** No. CopyCraft works fully offline and has no network access.
5. **How do I clear what I copied?** Use Wipe or Clear history on the card or in the menu.
6. **How do I take a screenshot of the card?** CopyCraft blocks that by default. Choose "Allow screenshots" in the menu to allow it for this session.
7. **An app should be ignored.** Add it under Ignored apps in Settings.
8. **How do I start CopyCraft at login?** Turn on "Open at Login" in Settings.

9. **How do I suggest a feature or report a problem?** Choose "Suggest a feature…" or "Report a problem…" in the ⋯ menu or in Settings. Fill in a subject and your text and choose "Create email": your mail app opens with a draft to mac-apps-support@proton.me. You send the mail yourself; CopyCraft sends nothing. The mail holds only your own text plus the CopyCraft and macOS versions, never your clipboard, history, file names or settings. If your text is too long for an email link, or you have no mail app, you can copy the text to the clipboard and paste it into a mail yourself.

## Logs and diagnostics

CopyCraft logs events only: what happened, counts, sizes, types and error kinds. It never
logs clipboard or history text, file contents, file names or paths, decoder input or output,
regex patterns or table data.

**View the log in Console.** Open Console.app, select your Mac, click Start streaming and
search for `subsystem:nl.marcelkoopman.copycraft`. The categories are
`clipboard`, `table`, `decode`, `settings`, `window`, `login` and `worker`. Include Info and
Debug messages (Action > Include Info Messages / Include Debug Messages) to see everything.

**Export diagnostics.** In the `⋯` menu choose *Export diagnostics…* (*Diagnose
exporteren…*) and save the `.txt` file. It holds the app version, the macOS version, the
settings (values only) and the log lines of the last 24 hours of the current run, kept in
memory. Attach it to your support request.
