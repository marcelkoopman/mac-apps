# CopyCraft – App Store Connect review notes (draft)

Draft for the *Notes* field under App Review Information in App Store Connect. Every statement
below was checked against the code at the time of writing; check it again before submitting.
Not published on GitHub Pages: it is listed under `exclude:` in `docs/_config.yml`, like every
other file in `docs/` except `privacy.md` and `support.md`. (GitHub Pages also publishes Markdown
files without front matter, so leaving out the front matter is not enough.)

---

Thank you for reviewing CopyCraft.

**What it is.** CopyCraft is a menu bar app (no Dock icon) for looking at and working with what
you copy: text, code, JSON, tables, links and pictures. It runs fully offline: the app has no
network entitlement and makes no network connections. No account or sign-in is needed, and
there is no in-app purchase.

**First launch.** A welcome window appears once. It explains the hotkey and paste-first use, and
has a "Keep clipboard history" checkbox (off), an "Open at Login" checkbox (off) and a "Get
started" button. The welcome window can be shown again from the
menu bar menu ("Welcome Screen").

**Global shortcut.** Press Control-Option-Space (⌃⌥Space, on every keyboard) in any app to open
the CopyCraft card for the current clipboard; the menu bar icon opens it too. Control-Option-4
(⌃⌥4) takes a screenshot. With more than one input source macOS may use Control-Option-Space for
"Select next source in Input menu"; turn that off in System Settings › Keyboard › Keyboard
Shortcuts › Input Sources, or record another shortcut. A shortcut that cannot be registered is
shown as "Hotkey in use, pick another" in Settings. The shortcuts can be changed in Settings (a bare key, Shift
as the only modifier, or Option or Option-Shift as the only modifiers on any key but §, is
refused). It is registered as a regular global hotkey and needs no Accessibility or Input
Monitoring permission.

**Clipboard history and guideline 2.5.14.** CopyCraft does not monitor the clipboard by default.
It records nothing in the background unless the user explicitly opts in, and the user can always
see and stop it:

- **Off by default.** On a new install "Keep clipboard history" is off. CopyCraft then does not
  poll the pasteboard (not even its change count) and does not read it when the card opens. It
  reads the pasteboard only when the user presses ⌘V in the card, and works on what is pasted or
  dropped on the card. The card says "Paste here (⌘V) or turn on history in Settings".
- **Explicit opt-in.** History watching starts only after the user ticks "Keep clipboard history"
  in the welcome window or in Settings › Privacy; both say in one sentence what it does. (An
  existing install that already watched the clipboard keeps the setting on.)
- History is kept in memory only (at most 20 unpinned entries plus pins). It is never written to
  disk and is cleared on quit, Wipe, screen lock, sleep and user switch, and by default 15
  minutes after the last copy.
- With history on, the menu bar menu and the card's ⋯ menu have **Pause Recording** (no copy is read or kept
  until it is turned off again; it is not stored, so a restart turns it off) and **Ignore next
  copy** (the next copy from another app is not read). Settings › Privacy has **Ignored apps**: a
  copy made while one of those apps is in front is not read. With history off these controls are
  greyed out, because nothing is read on its own.
- With history on, unless the user sets CopyCraft to *Always Allow* under Paste from Other Apps, macOS's
  pasteboard privacy applies and CopyCraft reads a copy only when the user opens the card
  (hotkey or menu bar icon). In the background it only looks at the pasteboard's change count and
  type names, which read no content, and asks macOS which kind the copy is (link, number, email
  address, phone number, calendar event, postal address: `detectPatternsForPatterns:`, which
  reads no content and shows no prompt) so the menu bar tooltip can name it. With *Always Deny*
  it reads nothing and asks nothing.

**Clipboard access prompt.** CopyCraft reads the clipboard of other apps (on ⌘V in the card, and
with history on also on its own), so macOS may show its "Paste from Other Apps" privacy prompt.
Please choose **Allow**. To review history watching, tick "Keep clipboard history" in the welcome
window or in Settings › Privacy first. While the setting is Default or
Ask, CopyCraft reads a new copy only when the card is opened (so the prompt appears when you open
the card, once per copy under Ask). To avoid repeated prompts during review, set CopyCraft to
*Always Allow* in System Settings › Privacy & Security › Paste from Other Apps. With *Always
Deny*, the card shows "Clipboard access denied in Privacy & Security" and nothing is read.

**Screenshots.** By default the CopyCraft windows are excluded from screenshots and screen
recordings (window sharing type "none"), because the card shows copied, possibly sensitive data.
To capture the card, turn on **"Allow Screenshots"** in the menu bar menu (Dutch UI:
"Schermafbeeldingen toestaan"). It applies to the card and the table window immediately and to
other CopyCraft windows opened after that. It is off again at every launch and is never stored.

**Apple Intelligence.** The AI features require Apple Intelligence. The AI actions (for example Summarize and Rewrite, under the card's *AI*
chip and *Text ▾* › Apple Intelligence) use Apple's on-device Foundation Models framework. They
need a Mac with Apple silicon, macOS 26 and Apple Intelligence turned on in System Settings.
Without that, the actions are shown dimmed with the reason (for example "Turn on Apple
Intelligence in System Settings"); the rest of the app works normally. No text is sent off the
device.

**Open at Login.** CopyCraft is not added as a login item unless the user turns on "Open at
Login" in the welcome window or in Settings (off by default). It uses `SMAppService`; when macOS
asks for approval, CopyCraft opens System Settings › General › Login Items.

**Privacy.** By default only what the user pastes (⌘V) or drops on the card is kept. History is kept in memory only and is cleared on quit, Wipe, screen lock, sleep and
user switch. Copies another app marks private (nspasteboard.org Concealed, Transient,
AutoGenerated types, as used by password managers) are not read and not kept; copies recognised
as tokens or keys (for example GitHub, AWS or Slack tokens and private keys) are shown but not
kept in history; so are images whose recognised text (on-device Vision text recognition)
contains one. Long texts (32 KB or more) and images are only added to history after that check. Settings are stored in the standard user defaults and never hold clipboard
content. "Suggest a Feature…" and "Report a Problem…" only open a draft in the user's own mail
app. The "Privacy Policy" and "Support" buttons (About window) open those
pages in the default browser.

**Sandbox entitlements.** `com.apple.security.app-sandbox` and
`com.apple.security.files.user-selected.read-write` (for "Choose File" and "Save"). Nothing else.

**System log.** CopyCraft writes diagnostic events (never clipboard content, file names or
paths) to the unified system log through `os_log_create` and `_os_log_impl`
(`crates/mac-ui/src/log.rs`). `_os_log_impl` is the function the `os_log` macro expands to; it is
declared in the public SDK header `<os/log.h>` and exported by libSystem. No private framework or
private API is used.

**Language.** The app is in Dutch and English and follows the macOS language by default
(Settings › Language).

**Contact.** mac-apps-support@proton.me

Privacy policy: https://marcelkoopman.github.io/mac-apps/privacy
Support: https://marcelkoopman.github.io/mac-apps/support
