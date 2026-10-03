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
| `detect/json_invalid.json` | Detection: invalid JSON (missing comma) | json | Invalid JSON | — | — | kind=Text |
| `detect/yaml.yaml` | Detection: YAML | yaml | YAML | — | — | kind=Yaml; convert=Json |
| `detect/xml_valid.xml` | Detection: valid XML | xml | Valid XML | — | Original, Schema | kind=Xml |
| `detect/xml_invalid.xml` | Detection: invalid XML (mismatched tag) | xml | Invalid XML | — | — | kind=Xml |
| `detect/html_page.html` | Detection: HTML with `<!DOCTYPE html>` | html | HTML | — | — | kind=Html |
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
| `detect/link_userinfo.txt` | Link with `user:pass@` (Visit strips it) | plain text | Link | credential | Visit | kind=Url; visit=https://example.com/private/report?id=7 |
| `detect/plain_text.txt` | Plain text | plain text | Content | — | — | kind=Text |
| `convert/yaml_to_json.yaml` | Convert YAML → JSON | yaml | YAML | — | — | convert=Json; convert~"app": "copycraft-demo" |
| `convert/key_value.txt` | Convert `key: value` → JSON | plain text | Content | — | Original, Convert | kind=Text; convert=Json; convert~"project": "Copycraft" |
| `convert/jwt.txt` | Decode the jwt.io example JWT | plain text | Content | credential | Original, Decode | decode~"name": "John Doe"; decode~"alg": "HS256" |
| `convert/base64.txt` | Decode Base64 | plain text | Content | — | Original, Decode | decode~Hello from Copycraft! This is synthetic test data. |
| `convert/data_uri.txt` | Decode a `data:` URI | plain text | Content | — | Original, Decode | decode~Copycraft data URI demo |
| `convert/percent.txt` | Decode percent-encoding | plain text | Content | — | Original, Decode | decode~https://example.com/search?q=copy%20craft&lang=en |
| `convert/json_to_avro.json` | JSON → Avro schema | json | Valid JSON | — | Original, Schema | schema~"type": "record"; schema~"name": "order_id" |
| `convert/xml_to_xsd.xml` | XML → XSD schema | xml | Valid XML | — | Original, Schema | schema~<xs:element name="order"> |
| `convert/xsd_to_sample.xsd` | XSD → sample XML | xml | Valid XML | — | Original, Sample | sample~<customer>sample</customer> |
| `tables/header_on_line_3.csv` | Table: title and Definition line above the header | plain text | CSV | — | Original, Dataframe, Table ▾ | meta~Header on line 3; read=4x4; notes=Header on line 3 |
| `tables/dates_ambiguous.csv` | Table: dates that fit dd/mm and mm/dd | plain text | CSV | — | Original, Dataframe, Table ▾ | read=5x2; notes=Dates read as dd/mm/yyyy; dtypes=date,i64; dataframe~2026-02-01 |
| `tables/dates_day_over_12.csv` | Table: a date part over 12 (read as mm/dd) | plain text | CSV | — | Original, Dataframe, Table ▾ | read=5x2; notes=none; dtypes=date,i64; dataframe~2026-01-13 |
| `tables/dates_iso.csv` | Table: ISO dates and datetimes | plain text | CSV | — | Original, Dataframe, Table ▾ | read=4x3; notes=none; dtypes=date,datetime[μs],f64 |
| `tables/semicolon_decimal_comma.csv` | Table: `;` and decimal comma, for Fix types | plain text | CSV | — | Original, Dataframe, Table ▾ | read=4x4; notes=Dates read as dd/mm/yyyy; dtypes=date,str,str,i64; step:FixTypes:dtypes=date,str,f64,i64; step:FixTypes~24.95 |
| `tables/messy_table.csv` | Table: duplicate rows, empty rows and column, constant column | plain text | CSV | — | Original, Dataframe, Table ▾ | read=8x5; step:Dedupe=5x5; step:DropEmpty=6x4; step:DropConstant=8x4; step:DropEmpty>Dedupe>DropConstant=4x3 |
| `tables/wide_prefix_20_columns.csv` | Table: 20 columns sharing a name prefix | plain text | CSV | — | Original, Dataframe, Table ▾ | read=6x20; columns~… PV1 Generation; dfcard~20 columns · 6 rows |
| `tables/small_table.csv` | Table: Transpose and Value counts | plain text | CSV | — | Original, Dataframe, Table ▾ | read=4x3; step:Transpose=2x5; step:ValueCounts(color)=3x2; step:ValueCounts(color)~red |
| `tables/tsv_table.tsv` | Table: TSV (tab separated) | plain text | TSV | — | Original, Dataframe, Table ▾ | kind=Tsv; read=3x4; dataframe~Desk lamp |
| `sensitivity/credential_private_key.txt` | Credential: private-key header (fake body) | plain text | Content | credential | — | kind=Text |
| `sensitivity/credential_aws_key.txt` | Credential: AWS example key id | plain text | Content | credential | — | kind=Plain |
| `sensitivity/credential_github_token.txt` | Credential: GitHub token (synthetic, as in `corpus.rs`) | plain text | Content | credential | — | kind=Plain |
| `sensitivity/credential_stripe_key.txt` | Credential: Stripe-style example key | plain text | Content | credential | — | kind=Plain |
| `sensitivity/pii_email.txt` | PII: `example.com` email | plain text | Content | PII | — | kind=Plain |
| `sensitivity/pii_phone.txt` | PII: fake 06 number | plain text | Content | PII | — | kind=Plain |
| `sensitivity/pii_bsn.txt` | PII: test BSN 111222333 | plain text | Content | PII | — | kind=Plain |
| `sensitivity/pii_names_column.csv` | PII: a names column (`naam`) | plain text | CSV | PII | Original, Dataframe, Table ▾ | kind=Csv |
| `sensitivity/financial_iban.txt` | Financial: test IBAN | plain text | Content | financial | — | kind=Plain |
| `sensitivity/financial_card.txt` | Financial: test card 4111 1111 1111 1111 | plain text | Content | financial | — | kind=Plain |
| `sensitivity/financial_salary_column.csv` | Financial: a salary column (`salaris`) | plain text | CSV | financial | Original, Dataframe, Table ▾ | kind=Csv |
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
| `visit=U` | Visit opens `U` (userinfo stripped, as `menubar::visit_current`) |
| `convert=K` / `convert~S` | the Convert result is detected as `K` / contains `S` |
| `decode~S`, `schema~S`, `sample~S` | the Decode, Schema or Sample view contains `S` |
| `read=RxC` | the table reads as R rows × C columns |
| `notes=…` | the table's reading notes (`Header on line N`, `Dates read as …`), `none` for none |
| `dtypes=…` | the table's column types, in order |
| `dataframe~S` / `dfcard~S` | the Dataframe text / the Dataframe card contains `S` |
| `columns~S` | one shown column name (shared prefix shortened) is `S` |
| `step:OPS=RxC`, `step:OPS~S`, `step:OPS:dtypes=…` | after the Table ▾ steps `OPS` (joined by `>`): the shape, the frame contains `S`, the types |
| `mac-only` | Title, Labels and Chips are what macOS should show; Linux checks only the rest |
| `size=WxH` | the image decodes (as stored) to W × H pixels |
| `exif-orientation=N`, `exif-gps` | the JPEG carries EXIF Orientation N and a GPS block |

Table ▾ step names: `Dedupe` (Remove duplicate rows), `DropEmpty` (Remove empty rows and
columns), `DropConstant` (Remove constant columns), `FixTypes` (Fix types), `Transpose`,
`ValueCounts(column)`.

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
