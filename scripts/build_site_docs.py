#!/usr/bin/env python3
"""Build the site's privacy and support pages from docs/privacy.md and docs/support.md.

Each docs file holds a "## Nederlands" and a "## English" section; English goes to
site/<page>/ (the default, used by the app's About and Help links), Dutch to
site/nl/<page>/. Run it after editing either docs file.
With --check it only reports whether the pages are up to date (exit 1 if not).
Python standard library only.
"""
import html
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASE_URL = "https://marcelkoopman.github.io/mac-apps/"
PAGES = ("privacy", "support")
LANGS = {
    "en": {
        "section": "English",
        "dir": "",
        "nav": [("#features", "Features"), ("#download", "Download"), ("privacy/", "Privacy"), ("support/", "Support")],
        "footer": [("privacy/", "Privacy"), ("support/", "Support")],
        "desc": {
            "privacy": "CopyCraft privacy policy: no data collection, no network, history in memory only.",
            "support": "Support and FAQ for CopyCraft.",
        },
        "lang_label": "Language",
    },
    "nl": {
        "section": "Nederlands",
        "dir": "nl/",
        "nav": [("#features", "Functies"), ("#download", "Download"), ("privacy/", "Privacy"), ("support/", "Ondersteuning")],
        "footer": [("privacy/", "Privacy"), ("support/", "Ondersteuning")],
        "desc": {
            "privacy": "Privacybeleid van CopyCraft: geen gegevensverzameling, geen netwerk, geschiedenis alleen in het werkgeheugen.",
            "support": "Ondersteuning en veelgestelde vragen over CopyCraft.",
        },
        "lang_label": "Taal",
    },
}


def inline(text, up):
    text = html.escape(text, quote=False)
    text = re.sub(r"`([^`]+)`", r"<code>\1</code>", text)
    text = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", text)

    def link(m):
        label, url = m.group(1), m.group(2)
        if url in PAGES:
            url = f"../{url}/"
        return f'<a href="{url}">{label}</a>'

    return re.sub(r"\[([^\]]+)\]\(([^)]+)\)", link, text)


def split_pair(text, lang):
    """'A / B' -> A for nl, B for en; text without ' / ' is returned as is."""
    if " / " not in text:
        return text
    parts = text.split(" / ")
    if lang == "nl":
        return parts[0]
    prefix = parts[0].rsplit(" – ", 1)[0] + " – " if " – " in parts[0] else ""
    return prefix + parts[-1]


def body_html(md, lang, up):
    md = re.sub(r"^---\n.*?\n---\n", "", md, flags=re.S)
    blocks = md.strip().split("\n\n")
    out, section = [], None
    for block in blocks:
        lines = block.split("\n")
        joined = " ".join(lines)
        if joined.startswith("## "):
            section = joined[3:]
            continue
        if section is None:
            if joined.startswith("# "):
                out.append(f"<h1>{inline(split_pair(joined[2:], lang), up)}</h1>")
            elif ":" in joined and " / " in joined and "](" not in joined:
                label, date = joined.split(":", 1)
                out.append(f"<p>{inline(split_pair(label, lang) + ': ' + split_pair(date.strip(), lang), up)}</p>")
            continue  # the NL/EN jump links are replaced by the language switch
        if section != LANGS[lang]["section"]:
            continue
        if all(re.match(r"\d+\. ", l) for l in lines):
            items = "".join(
                f"\n                        <li>{inline(re.sub(r'^\d+\. ', '', l), up)}</li>" for l in lines
            )
            out.append(f"<ol>{items}\n                    </ol>")
        elif joined.startswith("### "):
            out.append(f"<h3>{inline(joined[4:], up)}</h3>")
        else:
            out.append(f"<p>{inline(joined, up)}</p>")
    return "\n                    ".join(out)


def lang_switch(lang, page_path):
    """EN | NL switch; page_path is relative to the site root, e.g. 'privacy/'."""
    depth = page_path.count("/") + (1 if lang == "nl" else 0)
    root = "../" * depth
    en = f"{root}{page_path}"
    nl = f"{root}nl/{page_path}"
    if lang == "en":
        return (f'<span aria-current="page">EN</span> | '
                f'<a href="{nl}" hreflang="nl" lang="nl">NL</a>')
    return (f'<a href="{en}" hreflang="en" lang="en">EN</a> | '
            f'<span aria-current="page">NL</span>')


def page_html(page, lang):
    cfg = LANGS[lang]
    md = open(os.path.join(ROOT, "docs", f"{page}.md"), encoding="utf-8").read()
    up_site = "../../" if lang == "nl" else "../"  # to site/
    up_lang = "../"  # to this language's home
    title = re.search(r"^# (.+)$", md, re.M).group(1)
    title = split_pair(title, lang)
    nav = "\n".join(f'                    <a href="{up_lang}{h}">{t}</a>' for h, t in cfg["nav"])
    footer = "\n                    ·\n".join(f'                    <a href="{up_lang}{h}">{t}</a>' for h, t in cfg["footer"])
    return f'''<!doctype html>
<html lang="{lang}">
    <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>{html.escape(title)}</title>
        <meta name="description" content="{html.escape(cfg["desc"][page])}" />
        <link rel="alternate" hreflang="en" href="{BASE_URL}{page}/" />
        <link rel="alternate" hreflang="nl" href="{BASE_URL}nl/{page}/" />
        <link rel="alternate" hreflang="x-default" href="{BASE_URL}{page}/" />
        <link rel="icon" href="{up_site}app-icon.png" />
        <link rel="stylesheet" href="{up_site}styles.css" />
    </head>
    <body>
        <header class="site-header">
            <div class="container nav">
                <a class="brand" href="{up_lang}">
                    <span class="brand-mark">CC</span>
                    CopyCraft
                </a>
                <nav>
{nav}
                </nav>
                <p class="lang-switch" aria-label="{cfg["lang_label"]}">
                    {lang_switch(lang, page + "/")}
                </p>
            </div>
        </header>

        <main>
            <section class="section">
                <div class="container narrow">
                    <!-- Generated by scripts/build_site_docs.py from docs/{page}.md; edit that file. -->
                    {body_html(md, lang, up_lang)}
                </div>
            </section>
        </main>

        <footer class="site-footer">
            <div class="container">
                <p>
                    © 2026 Marcel Koopman ·
                    <a href="https://github.com/marcelkoopman/mac-apps" target="_blank" rel="noopener">GitHub</a>
                    ·
{footer}
                </p>
            </div>
        </footer>
    </body>
</html>
'''


def main():
    check = "--check" in sys.argv[1:]
    stale = []
    for page in PAGES:
        for lang, cfg in LANGS.items():
            path = os.path.join(ROOT, "site", cfg["dir"], page, "index.html")
            want = page_html(page, lang)
            have = open(path, encoding="utf-8").read() if os.path.exists(path) else None
            if have == want:
                continue
            if check:
                stale.append(os.path.relpath(path, ROOT))
            else:
                os.makedirs(os.path.dirname(path), exist_ok=True)
                with open(path, "w", encoding="utf-8") as f:
                    f.write(want)
    if stale:
        print("site pages out of date, run scripts/build_site_docs.py: " + ", ".join(stale))
        sys.exit(1)


if __name__ == "__main__":
    main()
