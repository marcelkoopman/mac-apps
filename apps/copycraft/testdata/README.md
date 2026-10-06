# Copycraft test data

Synthetic inputs for every Copycraft scenario. Copy a file's contents (or the matching Notion
code block) and open Copycraft: the card should show what the manifest below says. Nothing
here is real: tokens are documented example or test values (AWS `AKIAIOSFODNN7EXAMPLE`,
a Stripe-style example key, the jwt.io example token, the test IBAN `NL91ABNA0417164300`,
the test card `4111 1111 1111 1111`, test BSN `111222333`, SSN `078-05-1120`, IPs from
`192.0.2.0/24`, `example.com` addresses).

The manifest is checked by `src/testdata_manifest.rs` (`cargo test -p copycraft
testdata_manifest`, runs on Linux). Each row describes the card for the text a Notion code
block copies: the file **without its final newline**. The file as committed (with the newline)
must get the same labels and pass the same checks; only its title and chips may differ:
copied with the newline, a link also gets a Format chip (Format drops the newline) and
`detect/rust_balanced.rs` is titled "Formatted Rust". Change the app or a file, and that test
tells you which row to update.
`cargo test -p copycraft testdata_manifest -- --ignored --nocapture` prints what the card
shows for each file today.

## Rules for the files

- Exact bytes matter: tabs in `tables/tsv_table.tsv`, a tab and spaces in
  `detect/python_mixed_tabs.py`, straight quotes everywhere. `.gitattributes` marks this
  folder `-text` so Git never converts line endings; do not let an editor reformat these files.
  When mirroring to Notion, paste the bytes as they are (no smart quotes, tabs kept).
- Notion has no CSV language: CSV and TSV blocks use `plain text`.
- Images are Notion image/file blocks, not code blocks (language `—`).

## Manifest

Columns (fixed; the test parses this table between the markers):

- **File**: path under `testdata/`.
- **Scenario**: what the file is for.
- **Notion**: Notion code-block language.
- **Title**: the card title (Original view).
- **Labels**: sensitivity labels on the card's meta line (`—`: none).
- **Chips**: the chips under the card, in order (`—`: none).
- **Checks**: more assertions, separated by `; ` (see [Checks](#checks)).

<!-- manifest:start -->
| File | Scenario | Notion | Title | Labels | Chips | Checks |
|------|----------|--------|-------|--------|-------|--------|
| `detect/json_valid.json` | Detection: valid JSON | json | Valid JSON | — | Original, Schema | kind=Json |
| `detect/json_invalid.json` | Broken JSON: missing comma | json | JSON · Invalid | — | — | kind=Json; meta~Line 5, column 3: expected `,` or `}`; errorline=5 |
| `detect/json_invalid_trailing_comma.json` | Broken JSON: trailing comma | json | JSON · Invalid | — | — | kind=Json; meta~Line 5, column 1: trailing comma; errorline=5 |
| `detect/json_invalid_missing_brace.json` | Broken JSON: missing `}` | json | JSON · Invalid | — | — | kind=Json; meta~Line 6, column 4: missing } or ]; errorline=6 |
| `detect/json_invalid_single_quotes.json` | Broken JSON: single quotes | json | JSON · Invalid | — | — | kind=Json; meta~Line 1, column 2: key must be a string in double quotes; errorline=1 |
| `detect/yaml_flow_unquoted.yaml` | Stays YAML: `{a: 1}` without quotes | yaml | YAML | — | Original, To JSON | kind=Yaml; json={"a":1,"b":["x","y"]} |
| `detect/yaml_quoted_values.yaml` | Stays YAML: a normal document with quoted values | yaml | YAML | — | Original, To JSON | kind=Yaml; json={"title":"Copycraft demo","tags":["clipboard","offline"],"settings":{"theme":"system","history":20}} |
| `detect/yaml.yaml` | Detection: YAML | yaml | YAML | — | Original, To JSON | kind=Yaml; convert=Json |
| `detect/xml_valid.xml` | Detection: valid XML | xml | Valid XML | — | Original, Schema | kind=Xml |
| `detect/xml_invalid.xml` | Detection: invalid XML (mismatched tag) | xml | Invalid XML | — | — | kind=Xml |
| `detect/html_page.html` | Detection: HTML with `<!DOCTYPE html>`, opens formatted (void `<meta>`/`<br>` do not indent) | html | HTML | — | — | kind=Html; pretty=detect/html_page.pretty.html |
| `detect/html_page.pretty.html` | Format: the expected pretty form of `html_page.html` (formats to itself) | html | HTML | — | — | kind=Html; pretty=detect/html_page.pretty.html |
| `detect/html_one_line.html` | Format: a whole HTML page on one line (void elements, style, pre, textarea, script, inline text in `<p>`) | html | HTML | — | — | kind=Html; pretty=detect/html_one_line.pretty.html |
| `detect/html_one_line.pretty.html` | Format: the expected pretty form of `html_one_line.html` (formats to itself) | html | HTML | — | — | kind=Html; pretty=detect/html_one_line.pretty.html |
| `detect/markdown.md` | Detection: Markdown | markdown | Markdown | — | — | kind=Markdown |
| `detect/rust_balanced.rs` | Detection: Rust, balanced | rust | Rust | — | — | kind=Rust |
| `detect/rust_unbalanced.rs` | Detection: Rust, extra `)` | rust | Rust · unbalanced ) | — | — | kind=Rust |
| `detect/java_missing_brace.java` | Detection: Java, class never closed | java | Java · missing } | — | — | kind=Java |
| `detect/python_strong.py` | Detection: Python with strong hints | python | Python | — | — | kind=Python |
| `detect/python_expected_indent.py` | Detection: Python, block without body | python | Python · expected indent | — | — | kind=Python |
| `detect/python_mixed_tabs.py` | Detection: Python, tab and spaces | python | Python · mixed tabs and spaces | — | — | kind=Python |
| `detect/python_one_liner.txt` | Detection: one Python-like line stays plain text | plain text | Content | — | — | kind=Plain |
| `detect/link.txt` | Link | plain text | Link | — | Visit | kind=Url; link=https://example.com/docs/getting-started; visit=https://example.com/docs/getting-started |
| `detect/youtube_link.txt` | YouTube link | plain text | YouTube | — | Visit | kind=Url; visit=https://www.youtube.com/watch?v=aBcDeFgHiJk |
| `detect/link_userinfo.txt` | Link with `user:pass@` (Visit strips it; the URL view leaves it out) | plain text | Link | credential | Visit, Original, URL | kind=Url; visit=https://example.com/private/report?id=7; decode~id = 7; decodetitle=URL; decodemeta~credential |
| `detect/plain_text.txt` | Plain text | plain text | Content | — | — | kind=Text |
| `convert/yaml_to_json_simple.yaml` | To JSON: simple YAML | yaml | YAML | — | Original, To JSON | kind=Yaml; convert=Json; json={"app":"copycraft-demo","features":["format","decode"]} |
| `convert/yaml_to_json_nested.yaml` | To JSON: nested YAML | yaml | YAML | — | Original, To JSON | kind=Yaml; convert=Json; json={"app":"copycraft-demo","version":3,"features":["format","convert","decode"],"owner":{"team":"demo","active":true}} |
| `convert/yaml_to_json_anchors.yaml` | To JSON: anchors, aliases, `<<` merge, non-string keys, a date | yaml | YAML | — | Original, To JSON | kind=Yaml; convert=Json; json={"defaults":{"host":"db.example.com","port":5432,"timeout":30},"development":{"host":"db.example.com","port":5432,"timeout":30,"database":"demo_dev"},"test":{"host":"db.example.com","port":5433,"timeout":30,"database":"demo_test"},"mirror":{"host":"db.example.com","port":5432,"timeout":30},"1":"numeric key","true":"boolean key","released":"2026-01-02"} |
| `convert/yaml_to_json_multi_document.yaml` | To JSON: two documents (`---`) become an array | yaml | YAML | — | Original, To JSON | kind=Yaml; convert=Json; json=[{"kind":"Service","metadata":{"app":"copycraft-demo"}},{"kind":"Deployment","spec":{"replicas":2}}] |
| `convert/yaml_invalid.yaml` | To JSON: invalid YAML gets no chip | yaml | Content | — | — | kind=Text |
| `convert/key_value.txt` | Convert `key: value` → JSON | plain text | Content | — | Original, Convert | kind=Text; convert=Json; convert~"project": "Copycraft" |
| `convert/jwt.txt` | Decode the jwt.io example JWT | plain text | Content | credential | Original, JWT | decode~"name": "John Doe"; decode~"alg": "HS256"; decodetitle=JWT; decodemeta~credential |
| `convert/base64.txt` | Decode Base64 | plain text | Content | — | Original, Base64 | decode~Hello from Copycraft! This is synthetic test data. |
| `convert/data_uri.txt` | Decode a `data:` URI | plain text | Content | — | Original, Base64 | decode~Copycraft data URI demo |
| `convert/percent.txt` | Decode percent-encoding | plain text | Content | — | Original, URL | decode~https://example.com/search?q=copy craft&lang=en; decode~q = copy craft |
| `decode/jwt_expired.txt` | JWT chip: test token (dummy secret), `exp` in the past | plain text | Content | credential | Original, JWT | decode~Signature not verified; decode~Expires (exp): expired (; decode~"name": "Copycraft Test"; decodetitle=JWT; decodemeta~credential |
| `decode/jwt_valid_until_2099.txt` | JWT chip: `exp` in 2099, still valid | plain text | Content | credential | Original, JWT | decode~Expires (exp): valid for; decode~Issued (iat):; decodemeta~credential |
| `decode/jwt_not_before_future.txt` | JWT chip: `nbf` in 2098, not valid yet | plain text | Content | credential | Original, JWT | decode~Not before (nbf): not valid yet (in |
| `decode/jwt_invalid_no_alg.txt` | No JWT chip: header without `alg` (still labelled by the secret detector) | plain text | Content | credential | — | kind=Plain |
| `decode/jwt_invalid_not_json.txt` | No JWT chip: three dotted parts that are not base64 JSON | plain text | Content | — | — | kind=Plain |
| `decode/base64_text.txt` | Base64 chip: UTF-8 text (padded, standard alphabet) | plain text | Content | — | Original, Base64 | decode~Second line: café ✓; decodetitle=Base64 |
| `decode/base64url_json_no_padding.txt` | Base64 chip: Base64URL JSON without padding, shown pretty | plain text | Content | — | Original, Base64 | decode~"roles": [ |
| `decode/base64_png.txt` | Base64 chip: a 3×2 PNG (type, pixels, bytes) | plain text | Content | — | Original, Base64 | decode~PNG image; decode~3 × 2 pixels |
| `decode/base64_gif.txt` | Base64 chip: a 5×4 GIF | plain text | Content | — | Original, Base64 | decode~GIF image; decode~5 × 4 pixels |
| `decode/base64_jpeg_data_uri.txt` | Base64 chip: an 8×6 JPEG as a `data:` URI | plain text | Content | — | Original, Base64 | decode~JPEG image; decode~8 × 6 pixels |
| `decode/base64_webp.txt` | Base64 chip: a 7×9 WebP | plain text | Content | — | Original, Base64 | decode~WebP image; decode~7 × 9 pixels |
| `decode/base64_heic_header.txt` | Base64 chip: a synthetic HEIC header (`ftyp heic`, `ispe` 4032×3024) | plain text | Content | — | Original, Base64 | decode~HEIC image; decode~4032 × 3024 pixels |
| `decode/base64_negative_binary.txt` | No chip: Base64 of random bytes | plain text | Content | — | — | kind=Plain |
| `decode/base64_negative_words.txt` | No chip: ordinary words (each line is checked on its own in `decode/base64.rs`) | plain text | Content | — | — | kind=Text |
| `decode/url_query.txt` | URL chip on a link: scheme, host, path and each query parameter decoded (`%xx`, `+`) | plain text | Link | — | Visit, Original, URL | decode~Path: /zoek/fietsen & zo; decode~q = rode fiets; decode~city = Den Haag; decode~Fragment: top; decodetitle=URL |
| `decode/url_encoded_whole.txt` | URL chip: a URL encoded as a whole, decoded and split | plain text | Content | — | Original, URL | decode~q = copy craft |
| `decode/url_negative.txt` | No chip: a link without query or escapes, `100% done`, prose, a `mailto:` (PII; each line checked in `decode/url.rs`) | plain text | Content | PII | — | kind=Text |
| `convert/json_to_avro.json` | JSON → Avro schema | json | Valid JSON | — | Original, Schema | schema~"type": "record"; schema~"name": "order_id" |
| `convert/xml_to_xsd.xml` | XML → XSD schema | xml | Valid XML | — | Original, Schema | schema~<xs:element name="order"> |
| `convert/xsd_to_sample.xsd` | XSD → sample XML | xml | Valid XML | — | Original, Sample | sample~<customer>sample</customer> |
| `tables/header_on_line_3.csv` | Table: title and Definition line above the header | plain text | CSV | — | Original, Open table | meta~Header on line 3; read=4x4; notes=Header on line 3 |
| `tables/dates_ambiguous.csv` | Table: dates that fit dd/mm and mm/dd | plain text | CSV | — | Original, Open table | read=5x2; notes=Dates read as dd/mm/yyyy; dtypes=date,i64; dataframe~2026-02-01; dfmeta~Dates read as dd/mm/yyyy |
| `tables/dates_day_over_12.csv` | Table: a day over 12 in the second part forces mm/dd/yyyy | plain text | CSV | — | Original, Open table | read=5x2; notes=Dates read as mm/dd/yyyy; dtypes=date,i64; dataframe~2026-01-13; dfcard~2026-01-13; dfmeta~Dates read as mm/dd/yyyy |
| `tables/dates_mixed_orders.csv` | Table: one date only fits dd/mm, another only mm/dd (stays text) | plain text | CSV | — | Original, Open table | read=3x2; notes=none; dtypes=str,i64; dataframe~13/01/2026 |
| `tables/dates_iso.csv` | Table: ISO dates and datetimes | plain text | CSV | — | Original, Open table | read=4x3; notes=none; dtypes=date,datetime[μs],f64 |
| `tables/semicolon_decimal_comma.csv` | Table: `;` and decimal comma, numbers typed as the table is read | plain text | CSV | — | Original, Open table | read=4x4; notes=Dates read as dd/mm/yyyy; dtypes=date,str,f64,i64; dfcard~24.95 |
| `tables/leading_zero_ids.csv` | Table: IDs with a leading zero (`007`) stay text when read | plain text | CSV | — | Original, Open table | read=3x2; dtypes=str,str; dfcard~007 |
| `tables/messy_table.csv` | Table: duplicate rows, empty rows and column (and a constant column, kept) | plain text | CSV | — | Original, Open table | read=8x5; step:Dedupe=5x5; step:DropEmpty=6x4; step:DropEmpty>Dedupe=4x4 |
| `tables/wide_prefix_20_columns.csv` | Table: 20 columns sharing a name prefix | plain text | CSV | — | Original, Open table | read=6x20; columns~… PV1 Generation; dfcard~┌ |
| `tables/join_orders.csv` | Table: orders (once the Join with example; now a plain table) | plain text | CSV | — | Original, Open table | read=4x3 |
| `tables/datetime_first_column.csv` | Table: datetimes like `2026-09-01 08:15` in the first column (the colon in the time is no `key: value`) | plain text | CSV | — | Original, Open table | kind=Csv; read=4x3; dtypes=datetime[μs],str,f64; dataframe~2026-09-01 08:15; dataframe~Greenhouse south |
| `tables/ledger_filter.csv` | Table: a datetime first column and negative amounts (once the Filter example) | plain text | CSV | financial | Original, Open table | read=5x3; dtypes=datetime[μs],str,f64 |
| `tables/small_table.csv` | Table: a small table | plain text | CSV | — | Original, Open table | read=4x3 |
| `tables/wide_six_columns.csv` | Table: six columns with long values, wider than the card (the first column stays in view while scrolling sideways, in the card and in Open table) | plain text | CSV | — | Original, Open table | read=4x6; frozen~│ Office chair ┆; dfcard~Trading Company; windowmeta~4 rows × 6 columns; window~Trading Company |
| `tables/window_payees_iban.csv` | Table: Open table on a sensitive table (IBANs): the window opens revealed and names the labels | plain text | CSV | financial | Original, Open table | read=3x4; windowmeta~3 rows × 4 columns; windowmeta~financial; window~Bob Jansen; windowcolumns=payee, iban, amount, paid on |
| `tables/sales_by_region.csv` | Table: text, whole numbers, decimals and dates, dates kept as dates | plain text | CSV | — | Original, Open table | read=6x5; dtypes=str,str,i64,f64,date |
| `tables/tsv_table.tsv` | Table: TSV (tab separated) | plain text | TSV | — | Original, Open table | kind=Tsv; read=3x4; dataframe~Desk lamp |
| `sensitivity/credential_private_key.txt` | Credential: private-key header (fake body) | plain text | Content | credential | — | kind=Text |
| `sensitivity/credential_aws_key.txt` | Credential: AWS example key id | plain text | Content | credential | — | kind=Plain |
| `sensitivity/credential_github_token.txt` | Credential: GitHub token (synthetic, as in `corpus.rs`) | plain text | Content | credential | — | kind=Plain |
| `sensitivity/credential_stripe_key.txt` | Credential: Stripe-style example key | plain text | Content | credential | — | kind=Plain |
| `sensitivity/pii_email.txt` | PII: `example.com` email | plain text | Content | PII | — | kind=Plain |
| `sensitivity/pii_phone.txt` | PII: fake 06 number | plain text | Content | PII | — | kind=Plain |
| `sensitivity/pii_bsn.txt` | PII: test BSN 111222333 | plain text | Content | PII | — | kind=Plain |
| `sensitivity/pii_names_column.csv` | PII: a names column (`naam`) | plain text | CSV | PII | Original, Open table | kind=Csv |
| `sensitivity/financial_iban.txt` | Financial: test IBAN | plain text | Content | financial | — | kind=Plain |
| `sensitivity/financial_card.txt` | Financial: test card 4111 1111 1111 1111 | plain text | Content | financial | — | kind=Plain |
| `sensitivity/financial_salary_column.csv` | Financial: a salary column (`salaris`) | plain text | CSV | financial | Original, Open table | kind=Csv |
| `sensitivity/leak_url_credentials.txt` | Leakguard: credentials in a URL | plain text | Content | credential | — | kind=Plain |
| `sensitivity/leak_ip_address.txt` | Leakguard: IPs from 192.0.2.0/24 | plain text | Content | PII | — | kind=Plain |
| `sensitivity/leak_mac_address.txt` | Leakguard: MAC address | plain text | Content | PII | — | kind=Plain |
| `sensitivity/leak_ssn.txt` | Leakguard: US SSN 078-05-1120 | plain text | Content | PII | — | kind=Plain |
| `sensitivity/none_amount.txt` | No label: amount 10.06 | plain text | Content | — | — | kind=Plain |
| `sensitivity/none_invalid_iban.txt` | No label: IBAN failing mod-97 | plain text | Content | — | — | kind=Plain |
| `sensitivity/none_card_fails_luhn.txt` | No label: card number failing Luhn | plain text | Content | — | — | kind=Plain |
| `edge/empty.txt` | Edge: empty input | plain text | Clipboard | — | — | placeholder~Nothing copied |
| `edge/unicode_emoji.txt` | Edge: Unicode and emoji | plain text | Content | — | — | kind=Text |
| `edge/long_line.txt` | Edge: one very long line (5,399 characters) | plain text | Content | — | — | kind=Plain; meta~5.4 KB |
| `images/qr_code.png` | Image: QR code ("Copycraft QR test 2026") | — | Image | — | Original, Info, QR, Image ▾ | mac-only; size=264x264 |
| `images/ocr_text.png` | Image: text for OCR | — | Image | — | Original, Info, Text, Image ▾ | mac-only; size=520x200 |
| `images/exif_rotated_gps.jpg` | Image: EXIF rotation and fake GPS, for Remove metadata | — | Image | — | Original, Info, Image ▾ | mac-only; size=320x240; exif-orientation=6; exif-gps |
<!-- manifest:end -->

### Checks

| Check | Meaning |
|-------|---------|
| `kind=K` | `format::detect` of the Notion copy (no final newline) is `FormatKind::K` |
| `meta~S` / `placeholder~S` | the card's meta line / placeholder contains `S` |
| `link=U` | the Link card shows `U` |
| `errorline=N` | the well marks line N (copied JSON that stops parsing there) |
| `visit=U` | Visit opens `U` (userinfo stripped, as `menubar::visit_current`) |
| `convert=K` / `convert~S` | the Convert result is detected as `K` / contains `S` |
| `json=J` | the Convert / To JSON result is the JSON value `J` (compact, key order free) |
| `pretty=FILE` | the Format view (what the card opens with) is exactly the testdata file `FILE` (without its final newline) |
| `frozen~S` | the frozen first column of the Dataframe grid (one line per grid line) contains `S` |
| `window~S` / `windowmeta~S` / `windowcolumns=S` | Open table: its grid / its meta line / its column sidebar (names joined with `, `) |
| `decode~S`, `schema~S`, `sample~S` | the Decode, Schema or Sample view contains `S` |
| `read=RxC` | the table reads as R rows × C columns |
| `notes=…` | the table's reading notes (`Header on line N`, `Dates read as …`), `none` for none |
| `dtypes=…` | the table's column types, in order |
| `dataframe~S` / `dfcard~S` / `dfmeta~S` | the Dataframe text / the Dataframe card / its meta line contains `S` |
| `columns~S` | one shown column name (shared prefix shortened) is `S` |
| `step:OPS=RxC`, `step:OPS~S`, `step:OPS:dtypes=…`, `step:OPS:meta~S` | after the Table ▾ steps `OPS` (joined by `>`): the shape, the frame contains `S`, the types, the version's meta line (as the table window shows it) contains `S`. Steps: `Dedupe`, `DropEmpty` |
| `mac-only` | Title, Labels and Chips are what macOS should show; Linux checks only the rest |
| `size=WxH` | the image decodes (as stored) to W × H pixels |
| `exif-orientation=N`, `exif-gps` | the JPEG carries EXIF Orientation N and a GPS block |

Table ▾ step names: `Dedupe` (Remove duplicate rows), `DropEmpty` (Remove empty rows and
columns).

## Large files (not committed)

```sh
sh apps/copycraft/testdata/generate_large.sh            # writes target/copycraft-testdata/
sh apps/copycraft/testdata/generate_large.sh /tmp/cc    # or any folder
```

- `large_1mb.csv`: about 1 MB (1,064,568 bytes), a 32,000-row CSV table. The card checks
  labels in the background ("Checking…") and previews the table.
- `large_over_8mb.txt`: 8,000,067 bytes, just over the 8 MB limit for an opened or dropped
  file: the card says "File is larger than 8 MB".

## Images

`images/make_images.py` regenerates the three images (python3 with Pillow and qrcode). QR and
OCR use macOS Vision, and the picture steps use ImageIO, so their Title/Chips are macOS
expectations; on Linux the test only checks size and EXIF.

## Manual checks (Mac only, no data)

- [ ] Copy a password from a password manager (it marks the copy concealed): the card shows
      "Hidden content", no text, no chips, and nothing is added to history.
- [ ] History is wiped when the Mac locks or sleeps: copy a few items, lock (⌃⌘Q) or sleep,
      unlock, and check that history is empty.
- [ ] `edge/empty.txt`: copying an empty Notion code block copies nothing, so the card keeps
      the previous copy. Copying blank text from an editor shows "Nothing copied".
- [ ] `images/qr_code.png`: the QR chip shows "Copycraft QR test 2026".
- [ ] `images/ocr_text.png`: the Text chip shows "Copycraft OCR test / Invoice 2026-0042 /
      Total 59.90 EUR".
- [ ] `images/exif_rotated_gps.jpg`: shown upright (arrow and "TOP" at the top); Image ▾ ›
      Remove metadata gives a version without GPS and camera data (check with Info or
      `exiftool` on a saved copy).
- [ ] Large files from `generate_large.sh`: open or drop them on the card.
- [ ] `tables/wide_six_columns.csv` › Open table: the window opens with the table
      visible (card may close); ⌘Tab away and back blurs it; a click reveals it again.
- [ ] In the window: Remove duplicate rows, then ↶/↷ and ⌘Z/⇧⌘Z and the version menu; the
      card follows. Resize, scroll sideways (Product stays visible), ⌘W closes.
- [ ] Reopen the window, then Wipe, lock (⌃⌘Q), sleep and Clear history: each closes it.
      Copying new items until its entry leaves history also closes it.
- [ ] `tables/window_payees_iban.csv` in the window: the meta line shows financial in red.
- [ ] Open table on any table opens the window; its Table ▾ shows exactly: Remove duplicate
      rows, Remove empty rows and columns, Choose columns…, Sort ascending ›, Sort descending ›. The
      window's Table ▾ has the two Remove steps and the two Sort submenus.
- [ ] There is no Dataframe chip and no Show columns chip: a table shows Original and Table ▾;
      clicking Table ▾ shows the grid with card title *Table* / *Tabel* (not Dataframe). Column
      picking is Table ▾ › Choose columns… in the table window (and still on the card while its picker is open). Esc, Cancel and click-outside always dismiss the
      picker; Apply with no change also closes. Find filters the rows; ⌘Z/⇧⌘Z step versions.
- [ ] Frozen pin width: on a multi-column table the second column is fully visible to the right
      of the pinned first column (not half-covered by the pin); no stray orange digits (e.g. "0)")
      above the header.
- [ ] Open table: the window opens with content visible (not blurred); Copy and Save are on the toolbar; the card may close.
- [ ] `tables/semicolon_decimal_comma.csv` (numbers and dates stored as text): the grid shows
      the date column as dates and the prices as numbers (24.95), with no step taken.
- [ ] `tables/leading_zero_ids.csv`: the id column shows `007` and `012` as copied (text, not 7 and 12).
- [ ] App icon: the Dock / About icon matches `assets/icon.icns` (or `AppIcon.icon` when present). On macOS 26+, the card, Settings and About show Liquid Glass (frosted on older macOS); chips look like system glass buttons.
- [ ] Language: set the system language to Dutch (Nederlands), restart Copycraft: menu bar shows *Instellingen…* / *Over Copycraft* / *Stop*, Settings and About are in Dutch; switch back to English and they follow.
- [ ] About Copycraft (menu bar or `⋯`): shows Copycraft, the version, the offline promise, and the GitHub URL as selectable text (not opened by the app).
- [ ] Settings window (menu bar *Settings…* or card `⋯`): change the hotkey to Control-Option-C (not Option-C alone — that is rejected); the tray tip and menu show the new label and the hotkey summons the card. Set date order to mm/dd/yyyy, copy a table with `01/02/2026`, and check the dates read month-first (chip still switches). Turn blur off: a masked well uses the shade instead of gaussian blur; turn it back on. Open at Login: switch on (may open System Settings › Login Items for approval); switch off.
- [ ] Pasteboard privacy, Always Deny (macOS with pasteboard privacy on; see
      `docs/pasteboard-privacy/README.md` for the developer flag): set *Privacy & Security ›
      Paste from Other Apps › Copycraft* to Deny, copy text and open the card: it says
      "Clipboard access denied in Privacy & Security" (not "Nothing copied"), no alert shows,
      and the log (Console or a terminal launch) has `pasteboard accessBehavior alwaysDeny`
      at start-up. Set it back to Allow (or Ask, after one alert): the card shows the copy without a new
      copy.
- [ ] Pasteboard privacy, first time under Ask (reset first: `tccutil reset Pasteboard
      <bundle id>` and `defaults delete <bundle id> CopycraftPasteAlertExplained`): copy text
      in another app: no alert while the card is closed, and the menu bar tooltip says "New
      copy — open the card to view". Open the card: it explains that macOS asks and that
      Allow shows the copy (Always Allow in Privacy & Security › Paste from Other Apps); then
      exactly one alert. Choose Allow: the card shows the copy. Quit, start again, open the
      card on a new copy: no explanation any more.
- [ ] Pasteboard privacy, Ask: copy text with the card closed and wait a few seconds: no
      alert. Open the card: exactly one alert; close and open it again: none. Copy a picture
      (a screenshot to the clipboard, ⌃⇧⌘4) and open the card: exactly one alert for it, with
      picture, Info and Text/QR chips and a history entry. Answer Don't Allow on a new copy:
      no further alert for it (also with the card open), one alert for the next copy.
- [ ] Pasteboard privacy, Always Allow: copies are read as they come (no alert, history fills
      with the card closed), as before.
- [ ] Pasteboard privacy, switch while running: with the card closed, switch Copycraft from
      Allow to Ask to Deny and back in Privacy & Security; after each, copy and open the card:
      it follows the setting (the log shows `pasteboard accessBehavior now …`).
- [ ] `tables/datetime_first_column.csv` (datetimes in the first column): copied from a text
      editor, the card shows CSV with Table ▾, `measured at` as a datetime.
- [ ] Out of screen capture: with the card open and the table window open (one revealed, one
      blurred), take a screenshot (⇧⌘3 and ⇧⌘4 › Space on each window), a screen recording
      (⇧⌘5) and share the screen in a video call: card and window should be absent. Note per
      tool and macOS version where they still show (ScreenCaptureKit on macOS 15.4+ is reported
      to ignore the setting); a screenshot of the card should show only what is behind it.
- [ ] Allow screenshots: in the menu bar menu turn on *Allow screenshots* (checkmark shows):
      a ⇧⌘4 › Space screenshot of the card (and of an open table window) shows it, still
      blurred until clicked. Turn it off: screenshots leave both out again. Turn it on,
      quit and start Copycraft: the checkmark is off and screenshots exclude the card again.
- [ ] Frozen column clipped + grid scrolls: a wide table (`tables/wide_six_columns.csv`, or a
      long wide CSV) in the Dataframe view — trackpad/wheel scroll right and down: other columns
      and rows move; the pinned first column stays inside the table area (never over the meta
      line or the chips). The same in Open in window (never over the version bar or below the
      grid).
