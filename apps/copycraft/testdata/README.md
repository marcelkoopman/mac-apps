# CopyCraft test data

Synthetic inputs for every CopyCraft scenario. Copy a file's contents (or the matching Notion
code block) and open CopyCraft: the card should show what the manifest below says. Nothing
here is real: tokens are documented example or test values (AWS `AKIAIOSFODNN7EXAMPLE`,
a Stripe-style example key, the jwt.io example token, the test IBAN `NL91ABNA0417164300`,
the test card `4111 1111 1111 1111`, test BSN `111222333`, SSN `078-05-1120`, IPs from
`192.0.2.0/24`, `example.com` addresses). The JWTs in `decode/` are test-only tokens signed (HS256) with the dummy secret `copycraft-test-only-not-a-secret`; the hashes there are digests of a synthetic sentence.

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
- **Title**: the card title (Original view). The card header shows the part before ` · ` (`Java`); the rest (`missing }`) leads the meta line.
- **Labels**: sensitivity labels on the card's meta line (`—`: none).
- **Chips**: the chips under the card, in order (`—`: none).
- **Checks**: more assertions, separated by `; ` (see [Checks](#checks)).

<!-- manifest:start -->
| File | Scenario | Notion | Title | Labels | Chips | Checks |
|------|----------|--------|-------|--------|-------|--------|
| `detect/json_valid.json` | Detection: valid JSON | json | Valid JSON | — | Text ▾, Schema | kind=Json |
| `detect/json_invalid.json` | Broken JSON: missing comma | json | JSON · Invalid | — | Text ▾ | kind=Json; meta~Line 5, column 3: expected `,` or `}`; errorline=5 |
| `detect/json_invalid_trailing_comma.json` | Broken JSON: trailing comma | json | JSON · Invalid | — | Text ▾ | kind=Json; meta~Line 5, column 1: trailing comma; errorline=5 |
| `detect/json_invalid_missing_brace.json` | Broken JSON: missing `}` | json | JSON · Invalid | — | Text ▾ | kind=Json; meta~Line 6, column 4: missing } or ]; errorline=6 |
| `detect/json_invalid_single_quotes.json` | Broken JSON: single quotes | json | JSON · Invalid | — | Text ▾ | kind=Json; meta~Line 1, column 2: key must be a string in double quotes; errorline=1 |
| `detect/yaml_flow_unquoted.yaml` | Stays YAML: `{a: 1}` without quotes | yaml | YAML | — | Text ▾, To JSON | kind=Yaml; json={"a":1,"b":["x","y"]} |
| `detect/yaml_quoted_values.yaml` | Stays YAML: a normal document with quoted values | yaml | YAML | — | Text ▾, To JSON | kind=Yaml; json={"title":"Copycraft demo","tags":["clipboard","offline"],"settings":{"theme":"system","history":20}} |
| `detect/yaml.yaml` | Detection: YAML | yaml | YAML | — | Text ▾, To JSON | kind=Yaml; convert=Json |
| `detect/xml_valid.xml` | Detection: valid XML | xml | Valid XML | — | Text ▾, Schema | kind=Xml |
| `detect/xml_invalid.xml` | Detection: invalid XML (mismatched tag) | xml | Invalid XML | — | Text ▾ | kind=Xml |
| `detect/html_page.html` | Detection: HTML with `<!DOCTYPE html>`, opens formatted (void `<meta>`/`<br>` do not indent) | html | HTML | — | Text ▾ | kind=Html; pretty=detect/html_page.pretty.html |
| `detect/html_page.pretty.html` | Format: the expected pretty form of `html_page.html` (formats to itself) | html | HTML | — | Text ▾ | kind=Html; pretty=detect/html_page.pretty.html |
| `detect/html_one_line.html` | Format: a whole HTML page on one line (void elements, style, pre, textarea, script, inline text in `<p>`) | html | HTML | — | Text ▾ | kind=Html; pretty=detect/html_one_line.pretty.html |
| `detect/html_one_line.pretty.html` | Format: the expected pretty form of `html_one_line.html` (formats to itself) | html | HTML | — | Text ▾ | kind=Html; pretty=detect/html_one_line.pretty.html |
| `detect/markdown.md` | Detection: Markdown | markdown | Markdown | — | Text ▾ | kind=Markdown |
| `detect/rust_balanced.rs` | Detection: Rust, balanced | rust | Rust | — | Text ▾ | kind=Rust |
| `detect/rust_unbalanced.rs` | Detection: Rust, extra `)` | rust | Rust · unbalanced ) | — | Text ▾ | kind=Rust |
| `detect/java_missing_brace.java` | Detection: Java, class never closed | java | Java · missing } | — | Text ▾ | kind=Java |
| `detect/python_strong.py` | Detection: Python with strong hints | python | Python | — | Text ▾ | kind=Python |
| `detect/python_expected_indent.py` | Detection: Python, block without body | python | Python · expected indent | — | Text ▾ | kind=Python |
| `detect/python_mixed_tabs.py` | Detection: Python, tab and spaces | python | Python · mixed tabs and spaces | — | Text ▾ | kind=Python |
| `detect/python_one_liner.txt` | Detection: one Python-like line stays plain text | plain text | Content | — | Text ▾ | kind=Plain |
| `detect/link.txt` | Link | plain text | Link | — | Open, Copy cleaned, Copy domain only, QR Code | kind=Url; link=https://example.com/docs/getting-started; visit=https://example.com/docs/getting-started |
| `detect/youtube_link.txt` | YouTube link | plain text | YouTube | — | Open, Copy cleaned, Copy domain only, QR Code | kind=Url; visit=https://www.youtube.com/watch?v=aBcDeFgHiJk |
| `detect/link_userinfo.txt` | Link with `user:pass@` (Open strips it; the URL view leaves it out) | plain text | Link | credential | Open, Copy cleaned, Copy domain only, URL | kind=Url; visit=https://example.com/private/report?id=7; decode~id = 7; decodetitle=URL; decodemeta~credential |
| `detect/plain_text.txt` | Plain text | plain text | Content | — | Text ▾ | kind=Text |
| `convert/yaml_to_json_simple.yaml` | To JSON: simple YAML | yaml | YAML | — | Text ▾, To JSON | kind=Yaml; convert=Json; json={"app":"copycraft-demo","features":["format","decode"]} |
| `convert/yaml_to_json_nested.yaml` | To JSON: nested YAML | yaml | YAML | — | Text ▾, To JSON | kind=Yaml; convert=Json; json={"app":"copycraft-demo","version":3,"features":["format","convert","decode"],"owner":{"team":"demo","active":true}} |
| `convert/yaml_to_json_anchors.yaml` | To JSON: anchors, aliases, `<<` merge, non-string keys, a date | yaml | YAML | — | Text ▾, To JSON | kind=Yaml; convert=Json; json={"defaults":{"host":"db.example.com","port":5432,"timeout":30},"development":{"host":"db.example.com","port":5432,"timeout":30,"database":"demo_dev"},"test":{"host":"db.example.com","port":5433,"timeout":30,"database":"demo_test"},"mirror":{"host":"db.example.com","port":5432,"timeout":30},"1":"numeric key","true":"boolean key","released":"2026-01-02"} |
| `convert/yaml_to_json_multi_document.yaml` | To JSON: two documents (`---`) become an array | yaml | YAML | — | Text ▾, To JSON | kind=Yaml; convert=Json; json=[{"kind":"Service","metadata":{"app":"copycraft-demo"}},{"kind":"Deployment","spec":{"replicas":2}}] |
| `convert/yaml_invalid.yaml` | To JSON: invalid YAML gets no chip | yaml | Content | — | Text ▾ | kind=Text |
| `convert/key_value.txt` | Convert `key: value` → JSON | plain text | Content | — | Text ▾, Convert | kind=Text; convert=Json; convert~"project": "Copycraft" |
| `convert/jwt.txt` | Decode the jwt.io example JWT | plain text | Content | credential | Text ▾, JWT | decode~"name": "John Doe"; decode~"alg": "HS256"; decodetitle=JWT; decodemeta~credential |
| `convert/base64.txt` | Decode Base64 | plain text | Content | — | Text ▾, Base64 | decode~Hello from Copycraft! This is synthetic test data. |
| `convert/data_uri.txt` | Decode a `data:` URI | plain text | Content | — | Text ▾, Base64 | decode~Copycraft data URI demo |
| `convert/percent.txt` | Decode percent-encoding | plain text | Content | — | Text ▾, URL | decode~https://example.com/search?q=copy craft&lang=en; decode~q = copy craft |
| `decode/jwt_expired.txt` | JWT chip: test token (dummy secret), `exp` in the past | plain text | Content | credential | Text ▾, JWT | decode~Signature not verified; decode~Expires (exp): expired (; decode~"name": "Copycraft Test"; decodetitle=JWT; decodemeta~credential |
| `decode/jwt_valid_until_2099.txt` | JWT chip: `exp` in 2099, still valid | plain text | Content | credential | Text ▾, JWT | decode~Expires (exp): valid for; decode~Issued (iat):; decodemeta~credential |
| `decode/jwt_not_before_future.txt` | JWT chip: `nbf` in 2098, not valid yet | plain text | Content | credential | Text ▾, JWT | decode~Not before (nbf): not valid yet (in |
| `decode/jwt_invalid_no_alg.txt` | No JWT chip: header without `alg` (still labelled by the secret detector) | plain text | Content | credential | Text ▾ | kind=Plain |
| `decode/jwt_invalid_not_json.txt` | No JWT chip: three dotted parts that are not base64 JSON | plain text | Content | — | Text ▾ | kind=Plain |
| `decode/base64_text.txt` | Base64 chip: UTF-8 text (padded, standard alphabet) | plain text | Content | — | Text ▾, Base64 | decode~Second line: café ✓; decodetitle=Base64 |
| `decode/base64url_json_no_padding.txt` | Base64 chip: Base64URL JSON without padding, shown pretty | plain text | Content | — | Text ▾, Base64 | decode~"roles": [ |
| `decode/base64_png.txt` | Base64 chip: a 3×2 PNG (type, pixels, bytes) | plain text | Content | — | Text ▾, Base64 | decode~PNG image; decode~3 × 2 pixels |
| `decode/base64_png_image.txt` | Base64 chip: a valid 4×4 PNG (fake, harmless) the card draws, not just describes | plain text | Content | — | Text ▾, Base64 | decode~PNG image; decode~4 × 4 pixels; decodetitle=Base64 |
| `decode/base64_gif.txt` | Base64 chip: a 5×4 GIF | plain text | Content | — | Text ▾, Base64 | decode~GIF image; decode~5 × 4 pixels |
| `decode/base64_jpeg_data_uri.txt` | Base64 chip: an 8×6 JPEG as a `data:` URI | plain text | Content | — | Text ▾, Base64 | decode~JPEG image; decode~8 × 6 pixels |
| `decode/base64_webp.txt` | Base64 chip: a 7×9 WebP | plain text | Content | — | Text ▾, Base64 | decode~WebP image; decode~7 × 9 pixels |
| `decode/base64_heic_header.txt` | Base64 chip: a synthetic HEIC header (`ftyp heic`, `ispe` 4032×3024) | plain text | Content | — | Text ▾, Base64 | decode~HEIC image; decode~4032 × 3024 pixels |
| `decode/base64_negative_binary.txt` | No chip: Base64 of random bytes | plain text | Content | — | Text ▾ | kind=Plain |
| `decode/base64_negative_words.txt` | No chip: ordinary words (each line is checked on its own in `decode/base64.rs`) | plain text | Content | — | Text ▾ | kind=Text |
| `decode/uuid_v4_braces.txt` | UUID chip: a GUID in braces, version 4, RFC variant (the braces make it read as a YAML flow mapping, so it also gets To JSON) | plain text | YAML | — | Text ▾, To JSON, UUID | decode~Version 4 (random); decode~Variant: RFC 9562 (RFC 4122); decodetitle=UUID |
| `decode/uuid_v1.txt` | UUID chip: version 1 with its 100 ns timestamp (2026-10-06 08:00:00.123 UTC) | plain text | Content | — | Text ▾, UUID | decode~Version 1 (time and node); decode~06/10/2026 08:00:00.123  UTC |
| `decode/uuid_v7_urn.txt` | UUID chip: `urn:uuid:` version 7 with its Unix-millisecond timestamp | plain text | Content | — | Text ▾, UUID | decode~Version 7 (Unix time); decode~06/10/2026 08:00:00.123  UTC |
| `decode/uuid_negative.txt` | No UUID: 32 hex without dashes, wrong groups, a `g`, inside a sentence, an open brace (each line checked in `decode/uuid.rs`) | plain text | Content | — | Text ▾ | kind=Text |
| `decode/hash_md5.txt` | Hash chip: 32 hex, probably MD5 (or a UUID without dashes) | plain text | Content | — | Text ▾, Hash | decode~Probably MD5; decode~Or a UUID without dashes; decodetitle=Hash |
| `decode/hash_sha1.txt` | Hash chip: 40 hex, probably SHA-1 | plain text | Content | — | Text ▾, Hash | decode~Probably SHA-1 |
| `decode/hash_sha224.txt` | Hash chip: 56 hex, probably SHA-224 | plain text | Content | — | Text ▾, Hash | decode~Probably SHA-224 |
| `decode/hash_sha256.txt` | Hash chip: 64 hex (upper case), probably SHA-256 | plain text | Content | — | Text ▾, Hash | decode~Probably SHA-256 |
| `decode/hash_sha384.txt` | Hash chip: 96 hex, probably SHA-384 | plain text | Content | — | Text ▾, Hash | decode~Probably SHA-384 |
| `decode/hash_sha512.txt` | Hash chip: 128 hex, probably SHA-512 | plain text | Content | — | Text ▾, Hash | decode~Probably SHA-512; decode~A hash cannot be reversed. |
| `decode/hash_negative.txt` | No hash: 31 and 33 hex, a `g`, digits only, letters only (each line checked in `decode/hash.rs`) | plain text | Content | — | Text ▾ | kind=Text |
| `decode/unix_seconds.txt` | Unix time chip: 10 digits (seconds) in local time, UTC, relative, ISO 8601 | plain text | Content | — | Text ▾, Unix time | decode~14/11/2023 22:13:20  UTC; decode~ago; decode~ISO 8601: 2023-11-14T22:13:20Z; decodetitle=Unix time |
| `decode/unix_milliseconds.txt` | Unix time chip: 13 digits (milliseconds) | plain text | Content | — | Text ▾, Unix time | decode~06/10/2026 08:00:00.123  UTC; decode~Unix time in milliseconds |
| `decode/unix_negative_numbers.txt` | No chip: a phone-like `06…` number, out of range (2282), 9, 11 and 16 digits, grouped digits (each line checked in `decode/unix_time.rs`) | plain text | Content | PII | Text ▾ | kind=Text |
| `decode/url_query.txt` | URL chip on a link: scheme, host, path and each query parameter decoded (`%xx`, `+`) | plain text | Link | — | Open, Copy cleaned, Copy domain only, URL, QR Code | decode~Path: /zoek/fietsen & zo; decode~q = rode fiets; decode~city = Den Haag; decode~Fragment: top; decodetitle=URL |
| `decode/url_encoded_whole.txt` | URL chip: a URL encoded as a whole, decoded and split | plain text | Content | — | Text ▾, URL | decode~q = copy craft |
| `decode/url_negative.txt` | No chip: a link without query or escapes, `100% done`, prose, a `mailto:` (PII; each line checked in `decode/url.rs`) | plain text | Content | PII | Text ▾, Open link | kind=Text |
| `convert/json_to_avro.json` | JSON → Avro schema | json | Valid JSON | — | Text ▾, Schema | schema~"type": "record"; schema~"name": "order_id" |
| `convert/xml_to_xsd.xml` | XML → XSD schema | xml | Valid XML | — | Text ▾, Schema | schema~<xs:element name="order"> |
| `convert/xsd_to_sample.xsd` | XSD → sample XML | xml | Valid XML | — | Text ▾, Sample | sample~<customer>sample</customer> |
| `tables/header_on_line_3.csv` | Table: title and Definition line above the header | plain text | CSV | — | Open Table, Copy as JSON | meta~Header on line 3; read=4x4; notes=Header on line 3 |
| `tables/dates_ambiguous.csv` | Table: dates that fit dd/mm and mm/dd | plain text | CSV | — | Open Table, Copy as JSON | read=5x2; notes=Dates read as dd/mm/yyyy; dtypes=date,i64; dataframe~2026-02-01; dfmeta~Dates read as dd/mm/yyyy |
| `tables/dates_day_over_12.csv` | Table: a day over 12 in the second part forces mm/dd/yyyy | plain text | CSV | — | Open Table, Copy as JSON | read=5x2; notes=Dates read as mm/dd/yyyy; dtypes=date,i64; dataframe~2026-01-13; dfcard~2026-01-13; dfmeta~Dates read as mm/dd/yyyy |
| `tables/dates_mixed_orders.csv` | Table: one date only fits dd/mm, another only mm/dd (stays text) | plain text | CSV | — | Open Table, Copy as JSON | read=3x2; notes=none; dtypes=str,i64; dataframe~13/01/2026 |
| `tables/dates_iso.csv` | Table: ISO dates and datetimes | plain text | CSV | — | Open Table, Copy as JSON | read=4x3; notes=none; dtypes=date,datetime[μs],f64 |
| `tables/semicolon_decimal_comma.csv` | Table: `;` and decimal comma, numbers typed as the table is read | plain text | CSV | — | Open Table, Copy as JSON | read=4x4; notes=Dates read as dd/mm/yyyy; dtypes=date,str,f64,i64; dfcard~24.95 |
| `tables/leading_zero_ids.csv` | Table: IDs with a leading zero (`007`) stay text when read | plain text | CSV | — | Open Table, Copy as JSON | read=3x2; dtypes=str,str; dfcard~007 |
| `tables/messy_table.csv` | Table: duplicate rows, empty rows and column (and a constant column, kept) | plain text | CSV | — | Open Table, Copy as JSON | read=8x5; step:Dedupe=5x5; step:DropEmpty=6x4; step:DropEmpty>Dedupe=4x4 |
| `tables/wide_prefix_20_columns.csv` | Table: 20 columns sharing a name prefix | plain text | CSV | — | Open Table, Copy as JSON | read=6x20; columns~… PV1 Generation; dfcard~┌ |
| `tables/join_orders.csv` | Table: orders (once the Join with example; now a plain table) | plain text | CSV | — | Open Table, Copy as JSON | read=4x3 |
| `tables/datetime_first_column.csv` | Table: datetimes like `2026-09-01 08:15` in the first column (the colon in the time is no `key: value`) | plain text | CSV | — | Open Table, Copy as JSON | kind=Csv; read=4x3; dtypes=datetime[μs],str,f64; dataframe~2026-09-01 08:15; dataframe~Greenhouse south |
| `tables/ledger_filter.csv` | Table: a datetime first column and negative amounts (once the Filter example) | plain text | CSV | financial | Open Table, Copy as JSON | read=5x3; dtypes=datetime[μs],str,f64 |
| `tables/small_table.csv` | Table: a small table | plain text | CSV | — | Open Table, Copy as JSON | read=4x3 |
| `tables/wide_six_columns.csv` | Table: six columns with long values, wider than the card (the first column stays in view while scrolling sideways, in the card and in Open Table) | plain text | CSV | — | Open Table, Copy as JSON | read=4x6; frozen~│ Office chair ┆; dfcard~Trading Company; windowmeta~4 rows × 6 columns; window~Trading Company |
| `tables/window_payees_iban.csv` | Table: Open Table on a sensitive table (IBANs): the window opens revealed and names the labels | plain text | CSV | financial | Open Table, Copy as JSON | read=3x4; windowmeta~3 rows × 4 columns; windowmeta~financial; window~Bob Jansen; windowcolumns=payee, iban, amount, paid on |
| `tables/sales_by_region.csv` | Table: text, whole numbers, decimals and dates, dates kept as dates | plain text | CSV | — | Open Table, Copy as JSON | read=6x5; dtypes=str,str,i64,f64,date |
| `tables/tsv_table.tsv` | Table: TSV (tab separated) | plain text | TSV | — | Open Table, Copy as JSON | kind=Tsv; read=3x4; dataframe~Desk lamp |
| `sensitivity/credential_private_key.txt` | Credential: private-key header (fake body) | plain text | Content | credential | Text ▾ | kind=Text |
| `sensitivity/credential_aws_key.txt` | Credential: AWS example key id | plain text | Content | credential | Text ▾ | kind=Plain |
| `sensitivity/credential_github_token.txt` | Credential: GitHub token (synthetic, as in `corpus.rs`) | plain text | Content | credential | Text ▾ | kind=Plain |
| `sensitivity/credential_stripe_key.txt` | Credential: Stripe-style example key | plain text | Content | credential | Text ▾ | kind=Plain |
| `sensitivity/pii_email.txt` | PII: `example.com` email | plain text | Content | PII | Text ▾ | kind=Plain |
| `sensitivity/pii_phone.txt` | PII: fake 06 number | plain text | Content | PII | Text ▾ | kind=Plain |
| `sensitivity/pii_bsn.txt` | PII: test BSN 111222333 | plain text | Content | PII | Text ▾ | kind=Plain |
| `sensitivity/pii_names_column.csv` | PII: a names column (`naam`) | plain text | CSV | PII | Open Table, Copy as JSON | kind=Csv |
| `sensitivity/financial_iban.txt` | Financial: test IBAN | plain text | Content | financial | Text ▾ | kind=Plain |
| `sensitivity/financial_card.txt` | Financial: test card 4111 1111 1111 1111 | plain text | Content | financial | Text ▾ | kind=Plain |
| `sensitivity/financial_salary_column.csv` | Financial: a salary column (`salaris`) | plain text | CSV | financial | Open Table, Copy as JSON | kind=Csv |
| `sensitivity/leak_url_credentials.txt` | Leakguard: credentials in a URL | plain text | Content | credential | Text ▾ | kind=Plain |
| `sensitivity/leak_ip_address.txt` | Leakguard: IPs from 192.0.2.0/24 | plain text | Content | PII | Text ▾ | kind=Plain |
| `sensitivity/leak_mac_address.txt` | Leakguard: MAC address | plain text | Content | PII | Text ▾ | kind=Plain |
| `sensitivity/leak_ssn.txt` | Leakguard: US SSN 078-05-1120 | plain text | Content | PII | Text ▾ | kind=Plain |
| `sensitivity/none_amount.txt` | No label: amount 10.06 | plain text | Content | — | Text ▾ | kind=Plain |
| `locks/cargo_project.lock` | No label: the Cargo project's Cargo.lock (hex checksums, versions, registry URLs) | toml | TOML | — | Text ▾ | kind=Toml |
| `sensitivity/none_invalid_iban.txt` | No label: IBAN failing mod-97 | plain text | Content | — | Text ▾ | kind=Plain |
| `sensitivity/none_card_fails_luhn.txt` | No label: card number failing Luhn | plain text | Content | — | Text ▾ | kind=Plain |
| `edge/empty.txt` | Edge: empty input | plain text | Clipboard | — | — | placeholder~Paste here (⌘V) |
| `edge/unicode_emoji.txt` | Edge: Unicode and emoji | plain text | Content | — | Text ▾ | kind=Text |
| `edge/long_line.txt` | Edge: one very long line (5,399 characters) | plain text | Content | — | Text ▾ | kind=Plain; meta~5.4 KB |
| `diff/a.txt` | Two history copies, left side | plain text | Content | — | Text ▾ | kind=Text |
| `diff/b.txt` | Two history copies, right side (bravo removed, delta and echo added) | plain text | Content | — | Text ▾ | kind=Text |
| `diff/a.json` | JSON whose only difference from b.json is key order | json | Valid JSON | — | Text ▾, Schema | kind=Json |
| `diff/b.json` | Same object as a.json, keys sorted a then b | json | Valid JSON | — | Text ▾, Schema | kind=Json |
| `diff/c.json` | Same shape as a.json with a real value change | json | Valid JSON | — | Text ▾, Schema | kind=Json |
| `images/qr_code.png` | Image: QR code ("Copycraft QR test 2026") | — | Image | — | Info, QR, Image ▾ | mac-only; size=264x264 |
| `images/ocr_text.png` | Image: text for OCR | — | Image | — | Info, Text, Image ▾ | mac-only; size=520x200 |
| `images/exif_rotated_gps.jpg` | Image: EXIF rotation and fake GPS, for Remove Metadata | — | Image | — | Info, Image ▾ | mac-only; size=320x240; exif-orientation=6; exif-gps |
<!-- manifest:end -->

### Checks

| Check | Meaning |
|-------|---------|
| `kind=K` | `format::detect` of the Notion copy (no final newline) is `FormatKind::K` |
| `meta~S` / `placeholder~S` | the card's meta line / placeholder contains `S` |
| `link=U` | the Link card shows `U` |
| `errorline=N` | the well marks line N (copied JSON that stops parsing there) |
| `visit=U` | Open opens `U` (userinfo stripped, as `menubar::visit_current`) |
| `convert=K` / `convert~S` | the Convert result is detected as `K` / contains `S` |
| `json=J` | the Convert / To JSON result is the JSON value `J` (compact, key order free) |
| `pretty=FILE` | the Format view (what the card opens with) is exactly the testdata file `FILE` (without its final newline) |
| `frozen~S` | the frozen first column of the Dataframe grid (one line per grid line) contains `S` |
| `window~S` / `windowmeta~S` / `windowcolumns=S` | Open Table: its grid / its meta line / its column sidebar (names joined with `, `) |
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

Table ▾ step names: `Dedupe` (Remove Duplicate Rows), `DropEmpty` (Remove empty rows and
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

## Diff

The copy on screen has *Voeg toe aan diff* / *Add to Diff* in the `⋯` menu, above History. In History ▸ the same action is the ⌥ alternate of each row: hold ⌥ and the row reads *Voeg toe aan diff: …* / *Add to Diff: …*. No new button on the card. *Wissel A en B* / *Swap A and B* is on that `⋯` menu once two copies are chosen. A and B are prefixes on the history row and on the meta line.

- [ ] Copy `diff/a.txt` and `diff/b.txt`. Add both (*Voeg toe aan diff* / *Add to Diff*). The card shows a *Diff* chip beside *Origineel* / *Original* and grows wider. The well is two columns, A on the left and B on the right. Removed `bravo` is red on the left, added `delta` and `echo` are green on the right, unchanged lines are gray. The summary is `+2 / −1` lines.
- [ ] Copy `diff/a.json` and `diff/b.json` (same object, different key order). The summary says *Geen verschillen* / *No differences*. Both copies stay in the well, A on the left and B on the right, so the key order can be read. Copy still puts that “no differences” line on the clipboard.
- [ ] With *Use privacy filter* on, the diff is masked until reveal. If either copy is sensitive, it is masked until reveal even when the copies themselves were revealed. With the filter off, the diff shows directly, also for a sensitive copy: no cover to click.
- [ ] Copy on the diff puts `+` and `-` lines on the clipboard (ASCII plus and minus). The diff is not added to history.
- [ ] *Wissel A en B* / *Swap A and B* turns A and B around (left was old, right was new).
- [ ] A third *Add to Diff* replaces the oldest pick. *Verwijder uit diff* / *Remove from Diff* drops a pick. Wipe, lock or restart clears the selection.
- [ ] A picture's *Add to Diff* item is dimmed.
- [ ] `⋯` › *Geschiedenis* / *History* with two copies: the rows show the copies only, no separate diff rows. Hold ⌥: each row turns into *Add to Diff: <that row>*; let go and it is the copy again. Picking the ⌥ row adds that copy to the diff. *Wis geschiedenis* / *Clear History* and *Wis vastgezette items* / *Clear Pins* sit below a separator line.
- [ ] Compare hint: on a fresh install (or after `defaults delete nl.marcelkoopman.copycraft CopycraftStripCompareUsed`), with two or more copies the strip has one small grey line under it: *⌘-klik op een tegel om te vergelijken* / *⌘-click a tile to compare*, centred, not overlapping the cards. ⌘Y hides it with the strip. After the first ⌘-click compare the line goes blank while the card stays the same height. Close and reopen the card: the row is gone (the card is 16 pt shorter) and it stays gone after quitting and relaunching.
- [ ] Compare from the strip: copy `diff/a.txt`, then `diff/b.txt`, and open the card (it shows `b.txt`). ⌘-click the `a.txt` card in the strip: the card switches to the *Diff* view with A = `a.txt` (older, left) and B = `b.txt` (newer, right), the same result as *Add to Diff* twice (`+2 / −1`). Both strip cards have an orange *A* / *B* badge in the top-left corner; the accent frame stays on `b.txt`. ⇧-click is a plain click (it shows that copy). Esc ends the comparison (badges gone, the card shows `b.txt` again, still open); a second Esc closes the card. A plain click on a strip card also ends it and shows that copy. ⌘-click on a picture card does nothing. VoiceOver reads a compared card as *…, compared, side A* (Dutch: *vergelijking, kant A*) and the hint *⌘-click to compare with the copy on screen* (*⌘-klik om te vergelijken met de getoonde kopie*).

## Manual checks (Mac only, no data)

- [ ] Card theme: open a JSON card (keys, strings, numbers), keep it open and open Settings › *Weergave* / *Display*. Pick each entry of *Kaartthema* / *Card theme* in turn: *Systeem*, *Licht*, *Donker*, *Hoog contrast* | *Nacht*, *Sepia*, *Solarized licht*, *Solarized donker*, *Nord*, *Gruvbox donker* | *Dracula*, *Monokai*, *Catppuccin Mocha*, *Tokyo Night*, *Oceaan*, *Bos* (EN: *System*, *Light*, *Dark*, *High contrast* | *Night*, *Sepia*, *Solarized Light*, *Solarized Dark*, *Nord*, *Gruvbox Dark* | *Dracula*, *Monokai*, *Catppuccin Mocha*, *Tokyo Night*, *Ocean*, *Forest*). The pop-up shows a separator at each | and nothing else between items. When it's closed it shows the chosen name in full (*Catppuccin Mocha* not cut off). Reopening Settings selects the stored theme, including one after a separator such as *Bos*. The card changes at once, without closing it: the well takes the theme's opaque background, text and code colours, and the preview plate beside the pop-up matches. The pop-up and the preview fit on one row; *Symbols* and *Restore Defaults* share the last row and nothing is cut off.
- [ ] Card theme *System* follows macOS live: with the card open, switch System Settings › Appearance between Light and Dark. The well (glass) and its text follow immediately. On *Nacht* / *Night* (or any other fixed theme) the well stays the same when macOS switches.
- [ ] Card theme on other cards: a diff (green and red rows), a link (host bold, path dimmed), a CSV (headers in the accent), line numbers, and a table with its frozen first column (the pin has the theme's background) read well on *Hoog contrast*, on *Solarized licht* and on two colourful themes (*Dracula*, *Bos*). The welcome screen stays navy whatever the theme.
- [ ] Card theme on the strip and the table window: with history on and three copies, pick *Hoog contrast*, *Solarized licht* and *Dracula* in turn. The strip tiles take the theme's background (the tile of the copy on screen the selection colour), text and a dimmer type mark; a compare side (A/B) keeps its orange frame. Open a CSV in the table window: rows and stripes, the header strip and the frozen column take the theme, column names its accent; switching themes repaints the open table at once. *Systeem* brings back the system tint and colours.
- [ ] Card theme survives a restart (stored as `CopycraftCardTheme` in the user defaults, no clipboard data).
- [ ] Remove one copy: copy three different words (`een`, `twee`, `drie`) from TextEdit and open the card. The strip shows three cards; the one on screen and the one under the pointer show a small ✕ top right. Click ✕ on the middle card (`twee`): only that card disappears, `een` and `drie` stay, and paste in TextEdit still gives `drie` (the clipboard is untouched). ⌘Z brings `twee` back in the middle. VoiceOver reads the ✕ as *Verwijder uit geschiedenis* / *Remove from History*. Then click ✕ on the newest card (`drie`): it stays out of the strip while the clipboard still holds it. Copy `drie` again in TextEdit: it is back at the front.
- [ ] A removed copy is forgotten on lock: with one copy in history, remove it (✕ or ⋯ › *Remove from History*), lock the screen (⌃⌘Q) and unlock, open the card: ⌘Z brings nothing back. With *Keep History 5 Min After the Last Copy* (*Bewaar geschiedenis 5 min na de laatste kopie*, card `⋯`), pin one copy, remove another and wait more than 5 minutes: ⌘Z brings nothing back and the pin stays.
- [ ] Strip tile details: copy `een`, wait two minutes, copy `{"a": 1}` and open the card. Hover the older card: the tooltip says *Text · copied 2 min ago* (*Tekst · 2 min geleden gekopieerd*), no text of the copy and no ⌘-click hint. The ✕ is 20 pt. With VoiceOver on, focus the older card and press VO-⌘-Space: *Remove from History* takes it out (⌘Z puts it back), *Compare with Shown* opens the diff; the card on screen only has *Remove from History*. The one-time *⌘-click a tile to compare* line under the strip is unchanged.
- [ ] Strip size: with three or more copies the strip cards are about one and a half times the old size (138 × 78 pt, 92 × 52 before): the type mark and up to three lines of text, inset from the rounded edge, with the ✕ in the top-right corner clear of the text. On the compact card two cards show whole, on the normal one four, on the wide one five; the rest scrolls sideways. The card is taller by the larger strip, everything above it keeps its place, and nothing is cut off at the bottom. Hovering a card shows its ✕; a click on the ✕ removes that card, a click elsewhere on the card shows it.
- [ ] ⌫ on the card removes the copy on screen and the card shows the next one; ⌘Z puts it back. Type to search and press ⌫: it only edits the search. Remove copies until one is left, then *⋯ › Remove from History*: the card shows the empty-card hint, not the clipboard.
- [ ] The top right shows the kind, then a trash icon on its own, then one glass group of three symbols: the character button, ⋯ and ✕ (no *Tekens* text). The gap before the group is wider than the gaps inside it. Hovering it shows *Alles wissen: geschiedenis, pins en klembord* / *Clear All: history, pins and clipboard*; VoiceOver reads *Alles wissen* / *Clear All*. A click asks *Alles wissen?* / *Clear All?* first (with *Ask before clearing* on); *Cancel* keeps everything.
- [ ] Icon tooltips: hover each icon-only button. ⋯ says *Meer* / *More*, ✕ *Sluit (Esc, ⌃⌥Spatie)* / *Close (Esc, ⌃⌥Space)* (the hotkey set in Settings); the well's buttons say *Kopieer …*, *Bewaar … (⌘S)* / *Save … (⌘S)*, *Verberg* / *Hide*, *Toon regelnummers* / *Show Line Numbers* and *Open … in Voorvertoning*. Switch the language in Settings: the tooltips follow at once, the same words VoiceOver reads.
- [ ] Line numbers button does not blur: with the privacy filter on, copy a YAML list of five long lines (over 1 KB), open the card and reveal it. Hover the well's right edge: top to bottom *Toon regelnummers* / *Show Line Numbers*, the eye (*Verberg* / *Hide*), *Kopieer*, *Bewaar*. Click the numbers button a few times quickly, also from a debug build (`cargo run`, where the card is rebuilt slowly): line numbers go on and off, the button never leaves its place, the eye stays below it and the well is never blurred. Only the eye button blurs (and shows again); *Kopieer* copies and *Bewaar* opens the save panel.
- [ ] No black well while a text card is built: from a debug build (`cargo run`, where cards build slowly), copy a YAML list of five long lines (over 1 KB) and open the card. Click *Naar JSON* / *To JSON*: the YAML stays on the card until the JSON is there, then the JSON shows; the well is never empty or black and never says *Bezig…* / *Working…* in between. The same for a Text ▾ step (*UPPERCASE*, *Format JSON…*) and the line numbers button. With the privacy filter on, a Text ▾ result is still masked again as a new copy, and a result with a token (`ghp_` and 36 letters and digits) is still kept out of history.
- [ ] Short header: open `detect/java_missing_brace.java`. The header says only *Java* (no ⌃⌥Space after it); the meta line starts with *missing }*, then lines and size. A valid Rust file says *Rust*. Change the hotkey in Settings: ✕'s tooltip shows the new chord.
- [ ] Window titles: open Settings: the title bar says *Algemeen* / *General*; ⌘2 makes it *Privacy*, ⌘3 *Weergave* / *Display*. Copy the text of `tables/dates_iso.csv` and *Open Table*: the window is titled *Tabel* / *Table*. Choose `tables/dates_iso.csv` with ⌘O and *Open Table*: the title is `dates_iso.csv`. Welcome and About keep their titles.
- [ ] Chip symbols: a JSON card's *Tekst ▾* / *Text ▾* chip shows the `textformat` glyph (Aa), *Origineel* / *Original* a curved back arrow (not a photo), a Base64 copy's decoder chip `</>` (not an open lock). `⋯` shows *Pauzeer opname* with a plain pause glyph and *Over CopyCraft* with the app symbol (a rounded square, not the *i* of *Info*). Add two copies to the diff: *Wissel A en B* / *Swap A and B* is in `⋯` (two swapped rectangles), not a chip, also when the card has four chips. No item shows an empty gap where a symbol failed to load.
- [ ] Table window toolbar: open a CSV from `tables/` with *Open tabel*. The window has a native toolbar in its title bar (unified compact: one row with the title, no separate glass strip above the sidebar): *Tabel ⌄* as text with its menu (the same steps as before, a pick works, *Kies kolommen…* opens its dialog after the menu closes), then *Kopieer* and *Bewaar* as symbol buttons only (two pages, a tray with a down arrow), and at the right the search field; no ↶ *v1/1* ↷ group (the versions are the strip along the bottom, see *Table versions in the strip*). Hovering shows *Kopieer de getoonde tabel*, *Bewaar de getoonde tabel* style tooltips; VoiceOver reads the names. Right-click the toolbar: no *Customize Toolbar…*, no icon/text choice. Do a step (Remove Duplicate Rows): the version strip appears along the bottom. Make the window narrow: items that do not fit go under » with their names. Copy and Save are dimmed while *Working on the table…*. With the privacy filter on, switch apps and back: *Klik op de tabel om hem te tonen* shows in the toolbar between the buttons and the search field. Switch the language to English: titles and tooltips turn English. Wipe: the window closes; reopened, the menus hold the new table only.
- [ ] Clean URL on a link: copy `https://x.nl/p?utm_source=x&id=5#top`. The link card has a *Schoon URL op…* / *Clean URL…* chip. The dialog says *Parameters to remove: 1* and shows `https://x.nl/p?id=5#top`. *Apply to Clipboard* copies that (paste in TextEdit to check) as a new copy in the strip (no *v2/2* capsule), and ⌘Z shows the original link again (the clipboard keeps the cleaned one). Copy `https://x.nl/p?id=5#top` itself: no chip.
- [ ] Text ▾ sections: paste `{"a": 1}`. The chip reads *Text ▾* (*Tekst ▾* in Dutch). On macOS 14 and later the menu has the headers *Lines*, *Convert* (case style, Format JSON) and *Analysis* (only on code or AGENTS.md), each over its own items; on macOS 13 separators stand in their place. After one step, *Undo* sits under a separator without a header. *Table ▾* and *Image ▾* have no headers.
- [ ] History settings: Settings › *Privacy* shows, indented under *Keep clipboard history*, *Keep history* (*Bewaartermijn*) with *15 min after the last copy*, *Clear sensitive copies after 60 s* and *Ask before clearing*; *Display* no longer has *Ask before clearing*. Untick *Keep clipboard history*: the three dim and cannot be changed; tick it again. Pick *5 min after the last copy*: card `⋯` › *Keep History* shows 5 min checked. Untick *Clear sensitive copies after 60 s* in `⋯` with Settings open: the box in Settings unticks too. Click *Clear All* and tick *Don't ask again*: *Ask before clearing* unticks. The window fits on a 13" MacBook Air (480 × 526 pt).
- [ ] Agent items: open a repository's `AGENTS.md` (or copy it with a `## Build` heading and a `cargo test` line). Under *Analysis* the items read *Commands from AGENTS.md*, *Show Heading Outline* and *Check AGENTS.md* (Dutch: *Commando's uit AGENTS.md*, *Toon koppenoverzicht*, *AGENTS.md controleren*). *Commands from AGENTS.md* puts the commands on the clipboard as a new copy (the *Copied* toast; no capsule or step name), and ⌘Z goes back.
- [ ] Clean URL in text: copy a few lines of prose with two links, one with `?fbclid=1&page=2` and one without a query. *Text ▾ › Clean URL…* shows *Parameters to remove: 1*; after *Apply to Clipboard* only `fbclid=1&` is gone and everything else, line endings included, is unchanged. Prose without tracking parameters has no *Clean URL…* in *Text ▾*.
- [ ] Format JSON: paste `{"b":1,"a":[1,2]}`. *Text ▾ › Format JSON…* opens on *Format*; tick *Sort keys*: the preview starts `{`, `  "a": [`. *Apply to Clipboard*: the clipboard holds the indented JSON with `a` first, as a new copy (the *Copied* toast; no capsule or step name). Open it again, pick *Compress*: the clipboard is `{"a":[1,2],"b":1}`. ⌘Z twice is back at the copy.
- [ ] Format JSON on broken JSON: paste `{"a": 1,}` (a *JSON · Invalid* card). The preview says *Line 1, column 9: trailing comma* in red, and *Apply to Clipboard* leaves the clipboard as it was with that note on the card. Paste `{"a":1,"a":2}`: the preview says a key appears twice.
- [ ] Case style: paste `HTTPServer` and `user_id` on two lines. *Text ▾ › Case Style ›* is a submenu with seven styles; pick *camelCase*. The dialog has *camelCase* picked, says *Lines that change: 2* and shows `httpServer`, `userId`. *Apply to Clipboard*: the clipboard holds `httpServer` and `userId` on two lines; switch the pop-up to *snake_case* first and it is `http_server`, `user_id`. ⌘Z goes back. An empty line between them stays.
- [ ] Encoding: paste `foobar`. *Text ▾ › Encoding ›* (*Tekst ▾ › Codering ›*) is a submenu of eight items: the four *Encode…* items, a separator line, the four *Decode…* items. *Encode as Base64* gives `Zm9vYmFy` on the card and the clipboard; *Decode Base64* on that gives `foobar` again; ⌘Z steps back. Paste `a b&c`: *Encode for URL* gives `a%20b%26c`, *Encode as Hex* gives `6120622663`. (Copy without a final newline: encoding takes the text exactly as copied.) Paste `abc$` and pick *Decode Base64*: the meta line says *This isn't valid Base64* (NL *Dit is geen geldige Base64*), the card and clipboard keep `abc$`. *Decode Hex* on `ff` says *The decoded bytes aren't text*. Neither note contains the copied text.
- [ ] Hashes: copy `abc` (no final newline). *Text ▾ › Hash ›* (*Tekst ▾ › Hash ›*, its own submenu right below *Encoding ›*, not inside it) has *Calculate SHA-256*, *Calculate SHA-1 (Not for Security)* and *Calculate MD5 (Not for Security)* (NL *… (niet voor beveiliging)*). *Calculate SHA-256* gives `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad` on the card and the clipboard (the *Copied* toast; no capsule or step name), ⌘Z goes back. *SHA-1* gives `a9993e364706816aba3e25717850c26c9cd0d89d`, *MD5* `900150983cd24fb0d6963f7d28e17f72`. With history on, copy `mail me at jan.devries@email.nl` (a *PII* card) and pick *Calculate SHA-256*: the meta line says *Hash copied, not in history (the text is sensitive)*, the clipboard holds the hash concealed and the history has no new row.
- [ ] ISO 8601: copy `2026-10-22T14:03:05+02:00`: chip *ISO 8601* shows `1792670585` and
      `1792670585000`, then the local time and UTC. Copy `2026-10-22T14:03:05` (no zone): the
      chip shows *As local time (UTC+02:00)* and *As UTC*, each with its Unix time. `2026-02-30`
      gets no chip.
- [ ] New: card `⋯ › New ›` (*Nieuw ›*): *UUID v4* twice gives two different values with a `4` as
      the 13th digit; *UUID v7* starts with the time (two in a row sort in order); *Unix Time
      (Seconds)* matches `date +%s`; *ISO 8601 Now* shows the local time with its offset. Each
      value is a new card and on the clipboard (⌘V in another app pastes it).
- [ ] Copy a password from a password manager (it marks the copy concealed): the card shows
      "Hidden content", no text, no chips, and nothing is added to history.
- [ ] With history on, a long log with a token stays out of history: in Terminal run
      `{ for i in $(seq 2000); do echo "INFO request served in 12 ms"; done; echo "gh""p_aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5"; } > /tmp/long.log`
      (over 32 KB), then copy its contents, drop the file on the card and paste it with ⌘V:
      the card shows *credential*, and History has no entry for it.
- [ ] With history on, a screenshot of a token stays out of history: copy a screenshot
      (⌃⇧⌘4) of that `ghp_` line in Terminal; the card shows the picture, and after a few
      seconds History still has no entry for it. Dropping or pasting
      the same screenshot: also not in history.
- [ ] With history on, an ordinary screenshot (no token) is in History a few seconds after
      copying, dropping or pasting it, also `images/ocr_text.png`.
- [ ] Secret notice on CopyCraft's own screenshot: show that fake `ghp_` line in Terminal
      (`echo "gh""p_aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5"`), take a screenshot of it with
      *Take Screenshot…* (⌃⌥4): once, near the pointer, an orange triangle with *Screenshot
      bevat mogelijk een geheim* / *Screenshot may contain a secret* and below it *Op het
      klembord gemarkeerd als privé en niet in de geschiedenis bewaard* / *Marked private on
      the clipboard and not kept in history*, gone after about 3 s; no thumbnail, no History
      entry, and the notice never shows the token or the text. With VoiceOver on it says both
      lines. Console.app (process CopyCraft) and the log file have no token, text or kind.
      A screenshot of plain text (`images/ocr_text.png` opened in Preview): no notice. With
      *Pause Recording* on, the token screenshot again: no notice (only *Copied*). The same
      line tiny or turned on its side (so the 1 s check reads nothing and the full scan finds
      it): the thumbnail goes and the notice shows once, a few seconds later.
- [ ] ⌘V in the search field (type `/` on the card) and in the column picker's filter pastes
      the clipboard text into the field; ⌘V with search closed still pastes onto the card.
- [ ] Hidden Edit menu: in Settings › AI's prompt fields and the card's search field, ⌘X ⌘C ⌘V
      ⌘A and ⌘Z/⇧⌘Z work on the text; on the card without a field, ⌘Z still steps back
      through the card's versions. ⌘H hides CopyCraft and ⌘Q quits it (winit's app menu).
- [ ] Way back: hide the menu bar icon (System Settings › Menu Bar, or ⌘-drag it off), then
      double-click CopyCraft in Finder (or open it from Launchpad): the card opens in the middle
      of the screen. The welcome screen's last line says *Menu bar icon hidden? Open CopyCraft
      again from the Finder or with ⌃⌥Space.* (Dutch: *Is het menubalkpictogram verborgen? …*).
- [ ] Welcome screenshot line: open the welcome screen (status menu, or `defaults delete
      nl.marcelkoopman.copycraft hasSeenWelcome` and a restart): right under the open-hotkey
      line it says *Take a screenshot with ⌃⌥4: its text is recognised, secrets flagged.*
      (Dutch: *Maak een schermafbeelding met ⌃⌥4: de tekst wordt herkend, geheimen
      gemarkeerd.*), wrapped and not cut off; the window grows to fit. Set another screenshot
      hotkey in Settings › General (say ⌃⌥5), close and reopen the welcome screen: the line
      shows ⌃⌥5. Same with a changed open hotkey: both lines show the stored chords.
- [ ] Window menu: ⌘W closes and ⌘M minimises Settings, the welcome screen and About (each
      now has an active yellow button); ⌘, on the card opens Settings.
- [ ] History is wiped when the Mac locks or sleeps: copy a few items, lock (⌃⌘Q) or sleep,
      unlock, and check that history is empty.
- [ ] `edge/empty.txt`: copying an empty Notion code block copies nothing, so the card keeps
      the previous copy. Pasting blank text with ⌘V shows "Paste here (⌘V) or turn on history in Settings"; with history on,
      copying blank text from an editor shows "Paste (⌘V) or drop something here". Both empty
      cards show the link *Open Privacy Settings…* (NL *Open privacy-instellingen…*) one line
      below; it opens Settings on the *Privacy* tab.
- [ ] `images/qr_code.png`: the QR chip shows "Copycraft QR test 2026".
- [ ] `images/ocr_text.png`: the Text chip shows "Copycraft OCR test / Invoice 2026-0042 /
      Total 59.90 EUR".
- [ ] `images/exif_rotated_gps.jpg`: shown upright (arrow and "TOP" at the top); Image ▾ ›
      Remove Metadata gives a new copy without GPS and camera data (check with Info or
      `exiftool` on a saved copy).
- [ ] Large files from `generate_large.sh`: open or drop them on the card.
- [ ] `tables/wide_six_columns.csv` › Open Table: the window opens with the table
      visible (card may close); ⌘Tab away and back blurs it; a click reveals it again.
- [ ] In the window: Remove Duplicate Rows, then ⌘Z/⇧⌘Z and the cards of the version strip; the
      card follows. Resize, scroll sideways (Product stays visible), ⌘M minimises, ⌘W closes.
- [ ] Reopen the window, then Wipe, lock (⌃⌘Q), sleep and Clear History: each closes it.
      Copying new items until its entry leaves history also closes it.
- [ ] `tables/window_payees_iban.csv` in the window: the meta line shows financial in red.
- [ ] Open Table on any table opens the window; its Table ▾ shows exactly: Remove duplicate
      rows, Remove Empty Rows and Columns, Choose Columns…, Sort Ascending ›, Sort Descending ›,
      then *Copy as JSON* (*Kopieer als JSON*; *Copy as CSV* for a table read from JSON). The
      window's Table ▾ has the two Remove steps and the two Sort submenus.
- [ ] There is no Dataframe chip and no Show Columns chip: a table shows Original and Table ▾;
      clicking Table ▾ shows the grid with card title *Table* / *Tabel* (not Dataframe). Column
      picking is Table ▾ › Choose Columns… in the table window (and still on the card while its picker is open). Esc, Cancel and click-outside always dismiss the
      picker; Apply with no change also closes. Find filters the rows; ⌘Z/⇧⌘Z step versions.
- [ ] Frozen pin width: on a multi-column table the second column is fully visible to the right
      of the pinned first column (not half-covered by the pin); no stray orange digits (e.g. "0)")
      above the header.
- [ ] Open Table: the window opens with content visible (not blurred); Copy and Save are on the toolbar; the card may close.
- [ ] `tables/semicolon_decimal_comma.csv` (numbers and dates stored as text): the grid shows
      the date column as dates and the prices as numbers (24.95), with no step taken.
- [ ] `tables/leading_zero_ids.csv`: the id column shows `007` and `012` as copied (text, not 7 and 12).
- [ ] App icon (`assets/AppIcon.icon`: three layers, *board*, *clip* and *braces*, on a solid #07112D fill): in the Dock, ⌘Tab, Finder › Get Info and About the icon is the frosted board, the metal clip with its slot over the top edge and the cyan-to-blue `[ ]` braces on a dark navy tile, in the system's rounded mask with Liquid Glass on macOS 26 (no second rounded square or dark ring inside the mask, no baked-in drop shadow); it looks like `assets/icon.svg` (the flat copy) with glass added. Open `assets/AppIcon.icon` in Icon Composer: three groups, Board, Clip and Braces, each one layer, background solid #07112D, nothing in the dark, clear or tinted appearances. In a Finder list (16 pt) the clip and braces still read. On macOS 26+, the card, Settings and About show Liquid Glass (frosted on older macOS); chips look like system glass buttons.
- [ ] Bundle icon files: in a release or `scripts/build_store_pkg.sh` build, `CopyCraft.app/Contents/Resources` has `Assets.car` and `AppIcon.icns` and no `icon.icns`; `iconutil -c iconset CopyCraft.app/Contents/Resources/AppIcon.icns -o /tmp/cc.iconset && ls /tmp/cc.iconset` lists all ten sizes, `icon_16x16.png` to `icon_512x512@2x.png`; `/usr/libexec/PlistBuddy -c 'Print :CFBundleIconFile' CopyCraft.app/Contents/Info.plist` says `AppIcon`. The mounted DMG's volume icon is the same app icon. The welcome window still shows its feathered icon.
- [ ] Menu bar icon: an outlined clipboard (thin, even 1.5 pt strokes like the system icons next to it), its clip a separate tab with a small clear gap to the top edge, and `{ }` braces with a visible point in the middle; edges are smooth, not stair-stepped. It is as tall as the other status icons (18 pt), black in a light menu bar, white in a dark one, and follows a tinted or wallpaper-coloured menu bar (template image); while a menu is open it shows highlighted. Copy something: the blink shows the same clipboard filled solid with the braces cut out, then the outline again. On an @1x display (or System Settings › Displays at a non-Retina scale), the clip and the two braces can still be told apart.
- [ ] Language: set the system language to Dutch (Nederlands), restart CopyCraft: menu bar shows *Instellingen…* / *Over CopyCraft* / *CopyCraft stoppen*, Settings and About are in Dutch; switch back to English and they follow.
- [ ] About CopyCraft (menu bar or `⋯`): shows CopyCraft, the version, the offline promise, and the GitHub URL as selectable text (not opened by the app).
- [ ] Settings tabs (menu bar *Settings…* or card `⋯`): the window shows three tabs, *General*, *Privacy* and *Display* (Dutch: *Algemeen*, *Privacy*, *Weergave*; no *Notifications* tab), and fits on a 13" MacBook Air screen with nothing cut off on any tab. Press ⌃Tab three times: *Privacy*, *Display*, then *General* again; ⌃⇧Tab from *General*: *Display*; ⌘⇧] and ⌘⇧[ do the same (also with the cursor in *Symbols* or *Ignored apps*; plain Tab still moves the focus). Click each tab and press ⌘1…⌘3 (⌘4 does nothing): only that tab's settings show, and the window takes that tab's height: its top edge (title bar) stays where it is while the bottom moves, smoothly; with System Settings › Accessibility › Display › *Reduce motion* on it jumps at once. No tab has empty space under its last setting, and none is cut off. Pick *Display*, close Settings and open it again: *Display* is still selected; quit CopyCraft, start it and open Settings: still *Display*. Quit, run `defaults write nl.marcelkoopman.copycraft CopycraftSettingsTab -int 4` (an old Feedback tab), start and open Settings: *General*. Switch the language: the window reopens on the same tab with the tab titles in the new language. Turn *Keep clipboard history* off on *Privacy*: *Ignored apps* and *Add Frontmost App* are greyed out; on again, they are not. While the hotkey field says *Press a shortcut… (Esc cancels)*, switch tab: recording stops and the old hotkey is shown again. Type in *Symbols* (Display) and switch tab without pressing Return: the list is kept. On *Privacy*, hover *Clear sensitive copies after 60 s*: the tooltip ends with *A notification for a sensitive copy: General › Notifications › Sensitive copy.* (Dutch: *Een melding bij een gevoelige kopie: Algemeen › Meldingen › Gevoelige kopie.*). At the bottom of *General* is the *Notifications* (*Meldingen*) block: the heading, the four checkboxes *AI action done*, *PDF recognition done*, *Errors* and *Sensitive copy* in two rows, and *A notification never contains your text.*, then *Announce confirmations* with its note (no longer on *Display*); untick one, close and reopen Settings: still unticked.
- [ ] Settings › *General*: change the hotkey to Control-Option-C (not Option-C alone — that is rejected); the tray tip and menu show the new label and the hotkey summons the card. Set date order to mm/dd/yyyy, copy a table with `01/02/2026`, and check the dates read month-first (chip still switches). On *Privacy*, turn blur off: a masked well uses the shade instead of gaussian blur; turn it back on. Open at Login: switch on (may open System Settings › Login Items for approval); switch off.
- [ ] Blur default: on a fresh install (or after `defaults delete nl.marcelkoopman.copycraft CopycraftBlurMaskedWell`), Settings › *Privacy* › *Blur masked content* (*Gemaskeerde inhoud vervagen*) is unticked. Tick *Use privacy filter* (off by default, see below): a copied text, picture and table on the card are then masked by a flat, opaque cover with no shape showing through, in the card theme's background (with *System*: light grey, or dark grey in Dark Mode, not black), with *Click to reveal* centred on it (try *System* in light and dark, *Night*, *Sepia*, *High contrast*: the cover matches the well, no white or black plate on a themed card; with Reduce Transparency too). *Open Table* masked: blank under a cover with *Click the table to reveal it*. A click reveals as before. Tick the box: the card and table window blur instead, at once; quit and relaunch: still ticked. Untick it, relaunch: still unticked. Elsewhere in this checklist "blurred" means masked, which needs *Use privacy filter* ticked; with blur off it is the flat cover.
- [ ] Settings › *Privacy* › *Use privacy filter* (*Gebruik privacyfilter*): on a fresh install (or after `defaults delete nl.marcelkoopman.copycraft CopycraftPrivacyFilter`) the box is unticked and the blur box, indented 20 pt below it with its note, is dimmed. A copied text, a picture, a table, a dropped file and a Decode chip (`decode/base64_png_image.txt`) show directly, with no click to reveal; the card's table and *Open Table* show sharp; a history search hit shows a line of its text. The meta line still shows sensitivity labels, and a token still stays out of history: copy `ghp_` followed by 36 letters and digits from another app, then copy something else: the token is not in the history strip or history search. With Settings › *General* › *Notifications* › *Sensitive copy* (*Gevoelige kopie*, on by default) ticked and notifications allowed, that token copy also posts *Possibly sensitive text copied* (*Mogelijk gevoelige tekst gekopieerd*), with the privacy filter off as well as on; untick *Sensitive copy* and copy a new token: no notification. The notification comes only from watching the clipboard: with *Keep clipboard history* off, copy a new token (and paste it with ⌘V into the card): no notification, whatever *Sensitive copy* says. Wipe, *Clear History*, lock, *Allow Screenshots* and the pasteboard behaviour are unchanged. Tick it **on** while the card and table window are open and showing content: both are masked at once (hint, click to reveal) and nothing stays readable; a click reveals again; the blur box is enabled. Quit and reopen CopyCraft: still ticked. Untick it, quit and reopen: still unticked, all shown unmasked.
- [ ] Pasteboard privacy, Always Deny (macOS with pasteboard privacy on; see
      `docs/pasteboard-privacy/README.md` for the developer flag): set *Privacy & Security ›
      Paste from Other Apps › CopyCraft* to Deny, copy text and open the card: it says
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
- [ ] Pasteboard privacy, kind hint under Ask (or never asked): with the card closed, copy
      `https://example.org` in another app and hover the menu bar icon: the tooltip says *New
      copy: Link — open the card to view* (NL *Nieuwe kopie: Link — …*), and no alert shows.
      Copy `jan@example.org`: *Email address* (*E-mailadres*). Copy `+31 20 123 4567`: *Phone
      number* (*Telefoonnummer*). Copy `42`: *Number* (*Getal*). Copy `tomorrow at 3pm`:
      *Calendar event* (*Kalenderevent*). Copy a street address: *Postal address* (*Postadres*).
      Copy `hello`: the plain *New copy — open the card to view*. Under Always Deny or Always
      Allow there is no hint. Console (subsystem of CopyCraft) shows no detected kinds.
- [ ] Pasteboard privacy, Always Allow: copies are read as they come (no alert, history fills
      with the card closed), as before.
- [ ] Pasteboard privacy, switch while running: with the card closed, switch CopyCraft from
      Allow to Ask to Deny and back in Privacy & Security; after each, copy and open the card:
      it follows the setting (the log shows `pasteboard accessBehavior now …`).
- [ ] `tables/datetime_first_column.csv` (datetimes in the first column): copied from a text
      editor, the card shows CSV with Table ▾, `measured at` as a datetime.
- [ ] Out of screen capture: with the card open and the table window open (one revealed, one
      blurred), take a screenshot (⇧⌘3 and ⇧⌘4 › Space on each window), a screen recording
      (⇧⌘5) and share the screen in a video call: card and window should be absent. Note per
      tool and macOS version where they still show (ScreenCaptureKit on macOS 15.4+ is reported
      to ignore the setting); a screenshot of the card should show only what is behind it.
- [ ] Allow Screenshots: in the menu bar menu turn on *Allow Screenshots* (checkmark shows):
      a ⇧⌘4 › Space screenshot of the card (and of an open table window) shows it, still
      blurred until clicked. Turn it off: screenshots leave both out again. Turn it on,
      quit and start CopyCraft: the checkmark is off and screenshots exclude the card again.
- [ ] Frozen column clipped + grid scrolls: a wide table (`tables/wide_six_columns.csv`, or a
      long wide CSV) in the Dataframe view — trackpad/wheel scroll right and down: other columns
      and rows move; the pinned first column stays inside the table area (never over the meta
      line or the chips). The same in Open in Window (never over the version bar or below the
      grid).
- [ ] Symbols: open the card, click the character button (the *A* symbol left of ⋯; tooltip *Kopieer een teken* / *Copy a character*, VoiceOver *Speciale tekens* / *Special characters*), click €, and paste in TextEdit with ⌘V.
      € appears. The popover briefly says "Gekopieerd: €" / "Copied: €".
- [ ] That character is not in history.
- [ ] Click a few characters: those move to the front of the list (at most eight, no duplicates).
- [ ] Settings › *Weergave* / *Display* › *Tekens* / *Symbols*: add, remove and reorder characters, quit CopyCraft and start it again.
      The change is still there.
- [ ] Herstel standaard / Restore Defaults puts the standard list back.

## Menus (Mac only, no data)

- [ ] Shortcuts on menu rows: `⋯` shows *Kies bestand…* / *Choose File…* with ⌘O, *Instellingen…* / *Settings…* with ⌘, and the strip toggle with ⌘Y. *Tekst ▾* / *Text ▾* shows ⌘Z on *Ongedaan maken* / *Undo* and ⇧⌘Z on *Herhaal* / *Redo*; *Afbeelding ▾* / *Image ▾* has no undo or redo rows. With `⋯` open, press ⌘Y: the menu closes and the strip toggles once (not twice: it is not back where it was). With *Tekst ▾* open after two text steps, press ⌘Z: one step back, not two. With the menus closed the same chords still work on the card.
- [ ] Right-click on a strip card (and ⌃-click): *Zet vast* / *Pin* (*Maak los* / *Unpin* on a pinned one), *Voeg toe aan diff* / *Add to Diff* (dimmed on a picture), a line, *Verwijder uit geschiedenis* / *Remove from History*. Each acts on that card, also when another copy is on screen; pin it, and the card gets the pin mark. `⋯` › *Geschiedenis* / *History* and the ✕ still work.
- [ ] Open any CSV in the table window, right-click a cell (and ⌃-click): *Kopieer cel* / *Copy Cell*, *Kopieer rij* / *Copy Row*, *Kopieer kolom* / *Copy Column*. Paste each in TextEdit: the cell's whole value (also when the grid clips it with "…"), the row with tabs, the column one value per line without its name. With the privacy filter on and the grid blurred, the right-click shows no copy items.
- [ ] Dim, do not hide: copy a text, open *Tekst ▾* / *Text ▾* before any step: *Ongedaan maken* / *Undo* and *Herhaal* / *Redo* are there, dimmed. After one step *Undo* is on, *Redo* dimmed. *Afbeelding ▾* / *Image ▾* on a picture has no undo or redo rows at all (each step is a new copy). Command search lists undo/redo only when they can run. Open a big CSV in the table window and click *Tabel ▾* / *Table ▾* while it says *Working on the table…*: a menu with the steps dimmed opens (no beep).
- [ ] Icons: *Tekst ▾* / *Text ▾* shows an icon on *Hoofdletterstijl ▸* / *Case ▸*, *Codering ▸* / *Encoding ▸* and *Hash ▸*, none on the rows inside. *Afbeelding ▾* / *Image ▾* has no icons. In `⋯`, *Nieuw ▸* / *New ▸* has a plus (not the clock of *Geschiedenis ▸*). *Info* and *Over* / *About* have different icons; *AGENTS.md controleren* / *Check AGENTS.md* (on an AGENTS.md copy) and *Kies kolommen…* / *Choose Columns…* too.
- [ ] `⋯` › *Thema* / *Theme*: the 16 card themes in three groups with a line between them (basic, classic, colourful), a check mark on the current one. Pick *Nord*: the card repaints at once, the check mark moves, and Settings › *Weergave* / *Display* shows *Nord* in its pop-up. `⋯` › *Help*: *Welkomstscherm* / *Welcome Screen*, *Idee voorstellen…*, *Probleem melden…*, *Exporteer diagnose…*, *Controleer AI*, each opens what it did before; *Stop* / *Quit* stays at the bottom of `⋯`.
- [ ] Code: copy `select o.id, c.name from orders o left join customers c on c.id = o.customer_id where o.total > 10 and c.name like 'a%'` from TextEdit. *Text ▾ › Code ›* (*Tekst ▾ › Code ›*, under *Hash ›*) has *Format SQL* / *SQL opmaken* and *Minify SQL* / *SQL verkleinen*. Format gives a new copy with `SELECT` on its own line, `o.id,` and `c.name` indented below it, then `FROM orders o`, `LEFT JOIN customers c ON …`, `WHERE o.total > 10` and an indented `AND c.name LIKE 'a%'` (the string as copied); ⌘Z goes back. Minify puts it on one line again. Copy `name:   copycraft` + newline + `items: [one, two]`: *Code ›* has *Format YAML*, *YAML on One Line* (`{"name":"copycraft","items":["one","two"]}`) and, below a line, *YAML to JSON*. A YAML copy with a `# comment` gives *Nothing left: the text stays as it is.* for Format and One Line (the comment would be lost). A JSON copy has *JSON to YAML* (keys in the copied order); a JSON with a key twice does nothing. An XML copy (`<a> <b>x</b> </a>`) has *Format XML* and *Minify XML* (`<a><b>x</b></a>`); XML with `<![CDATA[` does nothing. A TOML copy and plain text have no *Code ›*. Copy a secret token inside YAML: the new copy gets the same label as any step.
- [ ] curl as code: copy (from TextEdit, over three lines with `\` at the line ends) `curl -X POST https://api.example.com/items \` / `-H 'Content-Type: application/json' \` / `--data-raw '{"name": "lamp"}'`. *Text ▾ › Code ›* has *As JavaScript (fetch)*, *As Python (requests)*, *As HTTPie*, *As Rust (reqwest)* and *As Swift (URLSession)* (NL *Als …*). Each gives a new copy with the same URL, method, header and body (`fetch(…, { method: "POST", …, body: JSON.stringify({…}) })`, `requests.post(…, json=json_data)`, `http --raw='…' POST …`, `.post(…).header(…).body(…)`, `URLRequest` with `httpMethod = "POST"`); ⌘Z goes back. Nothing is run and there is no network traffic. `curl -G https://e.com/s --data-urlencode q=lamp` puts `?q=lamp` in the URL; `-u demo:pw` gives Basic auth, `-F f=@a.txt` a multipart form. Copy `curl -H 'Authorization: Bearer fake-token' https://api.example.com/` and convert it: the new copy is concealed and stays out of history (the credential rule), also when the code no longer looks like a token. Plain text starting with `curly` has no curl items.
- [ ] Colour card: copy `#3366CC`. The card is titled *Kleur* / *Color*, with a blue swatch on top and the rows HEX `#3366CC`, RGB `rgb(51 102 204)`, HSL `hsl(220 60% 50%)`, OKLCH `oklch(53.2% 0.168 262.3)`, SwiftUI `Color(red: 0.2, green: 0.4, blue: 0.8)`, NSColor `NSColor(srgbRed: 0.2, green: 0.4, blue: 0.8, alpha: 1)`, *Contrast on white* `5.36:1 · AA ✓ · AAA ✗` and *Contrast on black* `3.91:1 · AA ✗ · AAA ✗`. A row can be selected and copied (⌘C). The same card for `#36c`, `rgb(51, 102, 204)`, `hsl(220deg 60% 50%)`; `rgba(255,0,0,0.5)` shows a see-through red swatch and `#FF000080`; `oklch(70% 0.4 145)` ends with *Outside sRGB: clipped to the nearest color*. `#123` (an issue number) and `the colour #3366CC` are plain text. With the card blurred, VoiceOver says *Clipboard content*; revealed it says *Color #3366CC*. Try the Light, Dark and a fixed card theme.
- [ ] Colour from the screen: *⋯ › New › Pick Color from Screen…* (NL *Kies kleur van scherm…*, eyedropper icon) shows the system loupe; click a pixel. The colour is on the clipboard as `#RRGGBB` and on a new Colour card titled *Color from Screen*; the card opens again if the loupe closed it. Esc in the loupe: nothing changes. No Screen Recording prompt.
- [ ] Cron: copy `0 8 * * 1`. The card has a *Cron* chip; it shows *Every Monday at 08:00* (NL *Elke maandag om 08:00*) and *Next times (local time):* with the next five Mondays at 08:00 and their UTC offset (the one after the end of summer time shows UTC+01:00). Also try `*/5 * * * *` (*Every 5 minutes*, the next five 5-minute marks from now), `30 9 * * 1-5` (*Monday through Friday at 09:30*), `0 0 1,15 * 1` (*… or every Monday …*), `@daily`, `@reboot` (*At startup*, no times), the six-field `0 0 8 * * ?` and `0 0 30 2 *` (*No next time in the coming eight years*). `1 2 3 4 5` (numbers only) has no Cron chip.
- [ ] Around the world: copy `1700000000` and `2026-10-22T14:03:05+02:00`. Under the local and UTC lines the decoder shows *Around the world:* (NL *Elders op de wereld:*) with Amsterdam, London, New York, San Francisco and Tokyo, each with its own UTC offset on that date (for the ISO time: Amsterdam 14:03:05 UTC+02:00, London 13:03:05 UTC+01:00, New York 08:03:05 UTC-04:00, San Francisco 05:03:05 UTC-07:00, Tokyo 21:03:05 UTC+09:00). A time without a zone (`2026-10-22T14:03:05`) shows its two readings, without the cities.
- [ ] QR code, make: copy `https://example.com/menu`. The link card has a *QR Code* chip (NL *QR-code*); click it. The clipboard now holds a large black-on-white QR code with a white margin, and the card shows it as a new picture copy titled *QR code*; the strip has it as the newest entry, ⌘V in Preview pastes it, and the iPhone camera reads the link from the screen. Copy a paragraph of prose: Text ▾ › *QR Code* does the same. Copy `export GITHUB_TOKEN=ghp_` plus 36 letters: no QR chip and no Text ▾ item (a QR code of a secret is the same secret).
- [ ] ✕ next to a made QR code: copy `https://github.com/marcelkoopman`, open the card and click the *QR Code* chip. The strip marks the new QR picture (blue frame), the same copy the big card shows; the link card is not marked. Hover the link card and click its ✕: the link is gone from the strip and from search (type `github`: nothing), the QR code stays on the card. Click ✕ on the marked QR card: it goes too and the big card shows the next copy, now marked. ⌘Z puts it back and shows it. Do the same with a dropped file (drag a `.txt` onto the card) and with plain text, link and picture cards: every ✕ removes its own card. The window's ✕ (top right, *Sluit* / *Close*) closes the card.
- [ ] ⌘-click compare next to a made copy: copy `eerste versie` in TextEdit, open the card and make a new value (⋯ › *Nieuw* / *New* › a UUID). The strip marks the new copy; ⌘-click the `eerste versie` card: the diff opens with `eerste versie` as A and the UUID as B, both cards carry their A/B badge. Esc ends the diff and the card shows the UUID copy. Drag a `.txt` file onto the card and ⌘-click an older text card: the same, the file's text as B. With a made QR code (or another picture) on the card, ⌘-click a text card: no diff (the diff compares text only), see the next step.
- [ ] ⌘-click compare with a picture: make a QR code (*QR Code* chip on a link card) and ⌘-click a text card: a short toast at the card, *Vergelijken werkt alleen met tekst* / *Compare only works with text*, in the style of *Gekopieerd* / *Copied*; no alert, the card keeps the QR code and stays usable, the toast goes by itself. The same with a screenshot or a copied picture on the card, and with text on the card when you ⌘-click a picture card. Two text cards still open the diff, without the toast. ⌘-click on the card already shown: nothing. Console.app (process `copycraft`) shows no text, kind or file name for it.
- [ ] QR code, read: on that QR picture's card (or a screenshot of any QR code) click the *QR* chip, then Copy. The code's text becomes a copy of its own: for a URL a link card titled *Text from QR code* (NL *Tekst uit QR-code*) with Open and the other link chips, in history and on the clipboard. A QR code holding a token is copied concealed, is shown on the card and stays out of history.
- [ ] Snippet, name: copy `Hoi {{naam}}, je bestelling {{order}} is op {{datum}} om {{tijd}} verstuurd ({{uuid}}). Groet, {{naam}}`, pin it, then ⋯ › *Bewaar als fragment…* / *Save as Snippet…*; name it `Verzending`. The strip and search show it as *Verzending*; ⋯ now has *Vul fragment in…* / *Fill In Snippet…* and *Hernoem fragment…* / *Rename Snippet…*. Rename to an empty name: it is plain pinned text again. Unpin: the name is gone. A picture or an unpinned copy has no *Save as Snippet…*.
- [ ] Snippet, fill in: name it again, open search and type `verz`: *Fragment: Verzending* / *Snippet: Verzending*. Pick it: a panel asks *naam* and *order* (once each, in that order); Tab goes from *naam* to *order*, the preview fills in as you type with today's date (NL `28-10-2026`, EN `2026-10-28`), the time as `14:05` and a UUID. Return makes a new copy: the card and the clipboard hold the filled text, the snippet stays as it was. Cancel makes nothing. A snippet `{{klembord}} gezien` with no fields makes its copy at once from what the clipboard held. Type a token (`ghp_` plus 36 letters) into a field: the copy is concealed and stays out of history, as any secret copy.
- [ ] Snippet, on disk: save a snippet, quit CopyCraft (⌘Q) and start it again: it is back in the strip as a pinned snippet with its name, and *Fill In Snippet…* works. `~/Library/Containers/nl.marcelkoopman.copycraft/Data/Library/Application Support/copycraft/snippets.json` exists with `-rw-------` (`ls -l`) and holds only the snippets, no other copies. *Wis geschiedenis* / *Clear History*, Wipe and locking the screen keep it; *Unpin* or *Remove from History* takes it out of the file (no snippet left: the file is gone). Pin a text and give it the name `ghp_` plus 36 letters: an alert says *Snippet bevat mogelijk een geheim en wordt niet bewaard* / *This snippet may contain a secret and is not saved*, and nothing is written. Replace the file with `{broken`: the next start has no snippets and does not crash.
- [ ] Verbs first: on a Rust card *Tekst ▾* / *Text ▾* shows *Meet complexiteit* / *Measure Complexity*; on an `AGENTS.md` card *Toon koppenoverzicht* / *Show Heading Outline*; *Hash ›* shows *Bereken SHA-256* / *Calculate SHA-256*, *Bereken SHA-1 (niet voor beveiliging)* / *Calculate SHA-1 (Not for Security)* and the same for MD5; with two copies in the diff `⋯` shows *Wissel A en B* / *Swap A and B*. Each does what it did before.

## Image ▾ steps (Mac only, no data)

Turn on *Klembordgeschiedenis bijhouden* / *Keep clipboard history*, then copy `images/ocr_text.png` (open it in Preview, ⌘A, ⌘C) and open the card.

- [ ] *Afbeelding ▾* / *Image ▾* › *Grijswaarden* / *Grayscale*: the card shows the grey picture with *Grayscale* in the meta line, and the *Gekopieerd* / *Copied* toast appears. There is no *1 van 2* / *1 of 2* (no *v1/2*, *v2/2*) capsule, no ↶ / ↷ and no undo or redo row in the menu.
- [ ] New copy: the strip has a new card at the left with the grey picture, next to the coloured one. Paste in Preview (*Nieuw van klembord* / *New from Clipboard*) or TextEdit: the grey picture.
- [ ] ⌘Z: the card shows the coloured picture again (that older strip card is marked); no toast, and pasting still gives the grey one. ⇧⌘Z: the grey one again. After ⌘Z, *Kopieer* / *Copy* puts the coloured one on the clipboard.
- [ ] A second step (*Spiegel verticaal* / *Flip Vertical*) on the grey copy makes a third copy; ⌘Z twice goes back to the coloured one. After ⌘Z, a step on the coloured one starts again from there (⇧⌘Z no longer goes to the flipped copy).
- [ ] Same as text: do a *Tekst ▾* step on a text copy: also a new copy and toast; ⌘Z shows the earlier copy without changing the clipboard (no *v2/2* capsule, as for a picture).
- [ ] `⋯` › *Pauzeer opname* / *Pause Recording*, then *Grayscale*: the grey picture is still put on the clipboard, the meta line says *Gekopieerd, niet in de geschiedenis (opnemen gepauzeerd)* / *Copied, not in history (recording paused)*, no new strip card, and ⌘Z does nothing. Resume recording afterwards.
- [ ] Sensitive: copy a screenshot that shows a fake token (`ghp_` followed by 36 letters and digits) and do *Grayscale*: the copy is made but not added to history (*… (holds a token or key)*), it is concealed (another clipboard manager does not record it) and the clipboard is emptied after 60 s.
- [ ] Console.app (filter `copycraft`): no file names, image bytes or recognised text in the log during these steps.

## Buttons (Mac only, no data)

- [ ] *Alles wissen?* / *Clear All?* (the trash button, with *Ask before clearing* on): *Annuleer* / *Cancel* is the blue default button and *Alles wissen* / *Clear All* is red. Return closes it as Cancel (history stays); open it again, Esc closes it as Cancel too. Only a click on the red button clears. Tick *Niet meer vragen* / *Don't ask again* and press Return or Esc: the setting stays on.
- [ ] Settings › *Weergave* / *Display* › *Herstel standaard* / *Restore Defaults*: first asks *Standaardsymbolen herstellen?* / *Restore Default Symbols?* with *Annuleer* as the default and *Herstel standaard* in red. Cancel (or Return, or Esc) keeps your own Symbols; the red button brings back the standard list.
- [ ] VoiceOver on a text card: the *Tekst ▾* / *Text ▾* chip reads "Tekst, menuknop" / "Text, menu button" (no "▾" or "down-pointing triangle"); on a picture *Afbeelding ▾* / *Image ▾* the same. VO-Space opens the menu once. The chips look as before. The table window's *Tabel* / *Table* toolbar menu reads the same way.
- [ ] Chip focus: turn on System Settings › Keyboard › *Keyboard navigation* (Full Keyboard Access). Open the card on a text copy and press ↓ / →: the selected chip gets the blue focus ring and keeps the accent; Return and Space both run that chip, once. Type a letter: the search opens with it; press ↓, then type another letter: it is added to the search. With Full Keyboard Access off, ↓ moves only the accent, as before.
- [ ] Dialog buttons at the regular size (28 pt, the same height as an alert's): the AI preview (*Annuleer*, *Pas toe*, minimise), the card's column picker (*Kies kolommen…* on a CSV: the four buttons along the bottom) and the table window's *Kies kolommen…* sheet. No title is cut off or overlaps; Settings rows keep their small buttons (*Herstel standaard*, *Voeg app vooraan toe*).
- [ ] Save button: VoiceOver on the card's save button (the down arrow in the well) reads *Bewaar…* / *Save…*; its tooltip still says *Bewaar JSON (⌘S)* and so on. The table window's toolbar *Save* still reads *Bewaar de tabel* / *Save the table*.
- [ ] Support icon: *Over CopyCraft* / *About CopyCraft* shows *Ondersteuning* / *Support* with the Safari compass, not a question mark; clicking it still opens the support page in the browser.
- [ ] About feedback: *Over CopyCraft* / *About CopyCraft* has *Idee voorstellen…* / *Suggest a Feature…* and *Probleem melden…* / *Report a Problem…* on a row above *Privacybeleid* / *Privacy Policy* and *Support*; nothing overlaps the GitHub address. Each mail button opens the same feedback window as `⋯` › *Help* ▸. Settings has no *Feedback* tab.
- [ ] Column picker labels: on a CSV card *Tabel* › *Kies kolommen…* shows *Selecteer alles* / *Select All* and *Selecteer niets* / *Select None*, uncut, on a Normal and a Wide card at the regular size; on a Compact card (`⋯` › *Venstergrootte* › *Compact*) the two are small, the whole row fits and nothing overlaps *Annuleer* / *Pas toe*. The table window's *Kies kolommen…* sheet shows the same two, uncut. ⌘A still selects all.

## History strip (Mac only, no data)

Copy four or five short texts and a small picture, then open the card so the strip shows.

- [ ] Click area: click a strip card on its very corner and on its padding next to the text or picture: it shows that copy, the same as a click in the middle. The ✕ appears over exactly the area that clicks (not in the 2 pt above or below the cards); the text and picture still keep their distance from the rounded edge.
- [ ] Strip motion: with five copies, click the ✕ on the third card: it fades out and the fourth and fifth slide left; nothing flickers or rebuilds. ⌘Z: the card fades back in and the others slide right. Copy something new with the card open: the new card fades in at the left. Turn on System Settings › Accessibility › Display › *Reduce motion*: the same steps jump without sliding or fading.
- [ ] Strip focus: with Full Keyboard Access on (System Settings › Keyboard › *Keyboard navigation*), select the second card and press ⌫: the focus ring goes to the card that slid into its place; on the last card it goes to the left neighbour. ⌘Z: the focus ring is on the card that came back. The same after a ✕ click and after *Remove from History* in a card's right-click menu.
- [ ] Diff badges: ⌘-click a second text card. Both cards show a small orange badge with *A* or *B* in their top-left corner, the type mark starts right of it, and neither card has an orange frame; the accent frame stays on the card shown, also after ←/→. VoiceOver on a compared card still says *vergelijking, kant A* / *compared, side A*. Try it with a fixed card theme too (Settings › *Weergave*).
- [ ] ⌘-click only: with three text copies, ⇧-click a strip card: it just shows that copy (no *Diff*). ⌘-click another card: the *Diff* view with badges. ⌘-click that same card again: it leaves the diff, its badge goes and the card shows the copy on screen again, as with *Remove from Diff* in its right-click menu. The hint under the strip and the tooltips say ⌘-click only.
- [ ] VoiceOver list: with VoiceOver on, move into the strip: it is announced as a list, *Geschiedenis* / *History*, with its number of items. Each card reads its type mark (and *vergelijking, kant A* while compared), no longer *Item 1 van 5, getoond*; the card on screen is read as selected, and after ←/→ the selection moves with the accent frame.
- [ ] Hover ✕: move the pointer over a card that is not on screen: its ✕ appears; move it off the strip (up into the card, or straight out of the window): the ✕ goes, only the marked card keeps one. Rest the pointer on a card and scroll the strip with two fingers without moving it: the ✕ moves to the card now under the pointer. With another app in front, hovering the strip still shows the ✕.
- [ ] Overflow: with more cards than fit (copy eight texts), the strip's right edge fades out over a finger's width, also on Liquid Glass and in light and dark; scroll to the last card: the fade goes; back: it returns. Try the compact, normal and wide sizes and the wide diff. Click a card through the faded edge: it opens. Step with → to a card past the edge: it stops just clear of the fade, its ✕ fully clickable.
- [ ] Pins: pin a text copy and a picture copy (right-click › *Zet vast* / *Pin*): both cards get a small pin bottom right, readable in light and dark and on the picture; the text stops before the pin; no `•` before the type mark any more. The cards keep their order. Unpin: the pin goes. VoiceOver on a pinned card: "…, Vastgezet" / "…, Pinned".
- [ ] Pins in the history menus: with a pinned and an unpinned copy, open the menu bar icon › *History ▸* (*Geschiedenis ▸*) and the card's `⋯` › *History*: the pinned row shows the filled pin symbol in front of its title (in the menu's text colour, white when the row is highlighted), the unpinned row has none, and no title starts with `•`. Pick the pinned row: that copy comes back as before. Unpin it (right-click its strip card › *Unpin*) and open both menus again: the pin is gone; pin it again: it is back. Type `/` and search for text in the pinned copy: its result chip shows the pin before the excerpt, and no `•`. VoiceOver on the pinned row in both menus and on the chip: "<title>, Vastgezet" / "<title>, Pinned". Hold ⌥ in `⋯` › *History*: the *Add to Diff* alternate of a pinned row has no pin.

## Privacy

Mac only, no data.

- [ ] Notifications refused, then allowed: turn CopyCraft's *Sta meldingen toe* / *Allow notifications* off in System Settings › *Meldingen* / *Notifications*. Open Settings › *Algemeen* / *General*: under *Meldingen* the note says *Meldingen staan uit voor CopyCraft* / *Notifications are off for CopyCraft*, the four switches are dimmed and the heading row has *Open Systeeminstellingen › Meldingen…* / *Open System Settings › Notifications…*, which opens that pane. Allow CopyCraft there, come back and switch to another tab and back to *Algemeen* (or close and reopen Settings): the note is back to *Een melding bevat nooit je tekst.*, the switches work and the button is gone.
- [ ] Refused, then allowed, without Settings: with CopyCraft refused, allow it in System Settings, then copy a fake token (`ghp_` and 36 letters and digits) from another app: the *Mogelijk gevoelige tekst gekopieerd* notification comes (no explanation dialog, no system question).
- [ ] Explanation before the system question: run `tccutil reset All nl.marcelkoopman.copycraft` in Terminal (this resets all of CopyCraft's permissions), quit and start CopyCraft, then trigger a notification (copy a fake token from another app with history on). The dialog is titled *Meldingen van CopyCraft* / *Notifications from CopyCraft* (no *toestaan* / *Allow*), keeps its text and has one button, *Doorgaan* / *Continue* (no *Geen meldingen* / *No Notifications*). Continue: the macOS question follows.
- [ ] Paste access denied (macOS 15.4+ with the pasteboard privacy preview on): set CopyCraft to *Weiger* / *Deny* under System Settings › *Privacy en beveiliging* / *Privacy & Security* › *Plakken vanuit andere apps* / *Paste from Other Apps*, copy something and open the card. Under its lines the card has the link *Open Privacy en beveiliging…* / *Open Privacy & Security…*, at the bottom of the well and not over the text (also on a Compact card). Click it: System Settings opens on *Paste from Other Apps* (or, if macOS does not know that anchor, on *Privacy & Security*). The empty card still has *Open privacy-instellingen…* / *Open Privacy Settings…*, which opens CopyCraft's own Settings › Privacy. Copy the link `x-apple.systempreferences:com.apple.preference.security` and choose *Openen* / *Open* (if offered) or click it in a card: it is still not opened.
- [ ] History wording: Settings › *Privacy*: under *Klembordgeschiedenis bijhouden* the note reads, on two lines and not cut off, *Leest elke kopie, alleen in het geheugen; weg bij stoppen of vergrendelen. macOS vraagt één keer of CopyCraft andere apps mag lezen.* (English: *Reads every copy, in memory only; gone when you quit or lock. macOS asks once whether CopyCraft may read other apps.*); nothing below overlaps it and the tab is a little taller than *Algemeen* (switching animates the height). The welcome window (quit, `defaults delete nl.marcelkoopman.copycraft hasSeenWelcome`, start again) says the same under its history checkbox, with the retention time (5, 15 or 60 min) and *Anders alleen wat je in de kaart plakt of neerzet.*
- [ ] Privacy policy: open `docs/privacy.md` (or the page after the next Pages build): the date is 28 October 2026; both languages have the *Fragmenten* / *Snippets* paragraph (the only content on disk, locally, never with secrets), the retention time (5/15/60 min, *Tot wissen* / *Until Cleared*), *Gevoelige kopieën na 60 s wissen* / *Clear sensitive copies after 60 s*, *Meldingen* / *Notifications* (local, never your text) and, under *Wat wel lokaal wordt opgeslagen* / *What is stored locally*, the temporary folder for a file dragged from Photos or Mail, deleted after reading. The Dutch history paragraph now names the ⌘Z copy like the English one. Drag a picture from Photos onto the card, then look in Finder at `~/Library/Containers/nl.marcelkoopman.copycraft/Data/tmp/` (or `$TMPDIR` without the sandbox): no `mac-ui-drop-…` folder is left once the card shows it.
- [ ] Apple Intelligence note: on a Mac with Apple Intelligence on, copy a paragraph of text and click the **AI** chip: the last line, dimmed and not clickable, reads *Draait op je Mac met Apple Intelligence; er gaat geen tekst naar een server.* (EN: *Runs on your Mac with Apple Intelligence; no text is sent to a server.*). Searching the card for "server" does not offer it. With Apple Intelligence off, the menu shows only the dimmed reason line as before.

## Drops and Image ▾ steps (Mac only, no data)

- [ ] Drop keeps the copied text: copy a line of text in TextEdit, open the card right away (before anything else), then drag a picture file from Finder onto it: the card shows the picture, and the strip has both the picture (at the front, at once) and the copied text behind it. Click the text card: it is there. Esc: the card shows the clipboard's text again. Repeat with a long text (a few pages, still being checked when you drop) and with *Pauzeer* / *Pause* on: paused, the text is not added (as before). A fake token (`ghp_` and 36 letters and digits) copied before the drop stays out of history, as before.
- [ ] A copy made from a dropped file is what the card shows next: drop `tables/wide_prefix_20_columns.csv` on the card and click *Kopieer als JSON* / *Copy as JSON*: the card shows the new JSON copy (title, preview and footer of the JSON, no file name), the same as clicking its tile, which is first and marked in the strip. The same with the file opened with ⌘O (*Kies bestand…* / *Choose File…*), with `/tmp/rows.json` (made below) and *Kopieer als CSV* / *Copy as CSV*, and on a dropped text file holding one link with *Kopieer opgeschoond* / *Copy cleaned*. From the table window (*Open tabel*) the card still shows the copy once the window closes.
- [ ] A drop left early still goes in: drag a large screenshot onto the card and press Esc at once (before its text is read): a few seconds later it is in the strip anyway. Drag two pictures one after the other: both end up in the strip. Wipe right after a drop: nothing comes back.
- [ ] Rotate at once: copy a screenshot (⌃⇧⌘4), open the card, *Afbeelding ▾* / *Image ▾* › *Draai rechtsom* / *Rotate Right*: within a second the card shows the turned picture, the strip has a new card at the front, and ⌘V in Preview (*Nieuw van klembord* / *New from Clipboard*) pastes the turned picture. *Draai linksom* / *Rotate Left* twice more: each click turns it a quarter back, at once. Do the same with *Grijstinten* / *Grayscale*, a flip, a resize and *Verwijder metadata* / *Remove Metadata*: each copy is there at once, no long spinner. ⌘Z steps back through them.
- [ ] Rotate a dropped picture: drag a picture file from Finder onto the card, then *Rotate Right*: the card shows the turned copy at once (it leaves the dropped file for the new copy), the strip has it.
- [ ] A step still checked afterwards: make a screenshot of a text editor showing a fake token (`ghp_` and 36 letters and digits) turned on its side (or with tiny text, so the first check finds nothing), copy it and rotate it upright: the turned copy appears at once; a few seconds later, when its text is read, it leaves the strip again (not kept, also not with ⌘Z) and the clipboard copy is marked private (a clipboard manager no longer records it; with *Gevoelige kopieën na 60 s wissen* / *Clear sensitive copies after 60 s* on, it is gone after a minute). A screenshot with an e-mail address on it, rotated: stays in the strip, the clipboard copy is marked private.
- [ ] A copy left by an earlier run: rotate a screenshot with *Image ▾* so CopyCraft's own copy is on the clipboard, quit CopyCraft (⌘Q) and start it again. Open the card: the picture has all its chips, as for a fresh copy: *Info*, *Afbeelding ▾* / *Image ▾* and, once its text is read (a few seconds), the text toggle (and *QR* for a barcode). It is in the strip and *Image ▾* › *Draai rechtsom* / *Rotate Right* works. Same with text: use a *Tekst ▾* / *Text ▾* step, quit, start again, open the card: the text has its usual chips and is in history. A copy that was marked private (a step on a screenshot with an e-mail address, then quit and start) shows *Verborgen inhoud* / *Hidden content*, like another app's private copy.
- [ ] A picture on the clipboard at launch gets *Image ▾* at once: copy a screenshot (⌃⇧⌘4), quit CopyCraft (⌘Q), start it again and open the card: *Info* and *Afbeelding ▾* / *Image ▾* are there on the first card, without waiting; the text toggle joins a few seconds later only if the picture has readable text (a picture turned on its side may have none). *Image ▾* › *Draai rechtsom* / *Rotate Right* works. Repeat with *Pauzeer* / *Pause* on before the copy, and with the copying app on the ignore list: the picture is not added to history, yet *Image ▾* is there and a step still makes a new copy (see below for which one history keeps). With history off (Settings › *Privacy*) the card shows only what was pasted or dropped, as before; ⌘V the picture into the card: *Image ▾* is there.
- [ ] If *Image ▾* is still missing (debug build, `cargo run -p copycraft`): in Terminal run `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"' | grep "image gate"`, start CopyCraft with the picture on the clipboard, open the card, and send the lines: they hold only yes/no flags (history kept, suppressed, image, in history, read, asked, scanned, refused), never content.
- [ ] A step on the picture from launch adds history: copy a screenshot, quit CopyCraft, start it again, open the card and choose *Afbeelding ▾* › *Draai rechtsom* / *Image ▾* › *Rotate Right*: the strip shows two cards, the turned copy first and the original after it; ⌘Z puts the original back on the clipboard. A second step adds a third card.
- [ ] A step on a picture already in history: copy a screenshot while CopyCraft runs (it is in the strip), rotate it: exactly one new card (the turned copy), not a second original; ⌘Z goes back.
- [ ] Ignored app: put the app you copy from (e.g. Preview) on the ignore list, copy a picture from it, open the card and rotate it: the original stays out of history, the turned copy is in it (you asked CopyCraft for it).
- [ ] *Pauzeer* / *Pause* on, then copy a picture and rotate it: neither the original nor the turned copy is added; the card says the copy was not kept. The clipboard holds the turned copy.
- [ ] Secret on a launch picture: copy a screenshot of a fake token (`ghp_` and 36 letters and digits), quit, start CopyCraft and rotate it at once: neither card stays in the strip (an original added before its text was read leaves again a few seconds later), the clipboard copy is marked private.
- [ ] If a step still adds no card (debug build): run the `log stream` command above with `grep "image step kept"` instead, do the step and send the line: only yes/no flags (source in history, clipboard copy, source kept, source checked, admitted, paused, held, kept), never content.
- [ ] Every text action is a new copy: copy `b`, `a`, `c` on three lines, open the card, *Tekst ▾* › *Keer regels om* / *Text ▾* › *Reverse Lines*: the *Gekopieerd* / *Copied* toast, the strip has a new card in front, and there is no *v2/2* capsule, no ↶ / ↷ and no *Undo Text Step* / *Redo Text Step* rows in the menu (nor in search). Do the same with *Encoding* › *Base64*, *Hash* › *SHA-256*, *Case style* › *snake_case*, *Filter (regex)…* › *Apply to Clipboard* and *Format JSON…* on `{"b":1}`: each adds one card.
- [ ] ⌘Z / ⇧⌘Z after a text step: ⌘Z shows the copy before the step on the card, ⇧⌘Z the result again; paste in TextEdit after each: the clipboard keeps the newest result. ⌫ a card in the strip, then ⌘Z: that card comes back first.
- [ ] Text on the clipboard at launch: copy a few lines, quit CopyCraft, start it again, open the card and reverse the lines: the strip shows two cards (the original after the result) and ⌘Z goes back to the original.
- [ ] Rules: with *Pauzeer* / *Pause* on, a step puts the result on the clipboard but adds no card, and the meta line says *Gekopieerd, niet in de geschiedenis (opnemen gepauzeerd)*. Ignore list, text and picture alike: add TextEdit to *Ignored apps* (Settings › *Privacy*), copy three lines in Notes (a card), click into a TextEdit window so it is in front, open the card with the hotkey and *Tekst ▾* › *Keer regels om*: the result is a new card in the strip (no "not in history" note), ⌘Z shows the lines before. Same with a screenshot (⌃⇧⌘4) and *Afbeelding ▾* › *Draai rechtsom* with TextEdit in front: a new card too. With *Pauzeer* / *Pause* on instead, neither adds a card. Reverse lines of a text with a fake token (`ghp_` and 36 letters and digits): no card, *… (bevat een token of sleutel)*. *Hash* › *SHA-256* of a text with an e-mail address: copied concealed, no card, the hash note.
- [ ] A dropped text file: drop a `.txt` on the card, *Tekst ▾* › *Keer regels om*: the card leaves the file for the new copy (in the strip); ⌘Z shows the file's text (its history entry from the drop).
- [ ] If a text step adds no card (debug build): run the `log stream` command above with `grep "text step kept"`, do the step and send the line: only yes/no flags (source in history, clipboard copy, source kept, admitted, paused, held, kept), never content.
- [ ] Drop after drop: drop a PNG from Finder onto the card, then drop a second picture right onto the shown picture: the card shows the second one (the outline appears while dragging) and both are in the strip. Then drop a `.txt` onto the picture, then a picture onto that text, and a picture dragged from Safari or Photos: each drop works, every time.

### Strip metadata from a dropped picture

- [ ] Location warning: drag `testdata/images/exif_rotated_gps.jpg` (fake GPS, made for the tests) from Finder onto the card. The meta line ends with *Bevat locatie* / *Contains location*, with no coordinates anywhere on the card. The chips show *Metadata verwijderen* / *Strip Metadata* just before *Afbeelding ▾* / *Image ▾*. Drop a screenshot PNG instead: the chip is there, the warning is not.
- [ ] Strip: on that JPEG, click *Metadata verwijderen* / *Strip Metadata*. Within a second the card shows the copy upright, with *Metadata verwijderd* / *Metadata removed* on the meta line, and no location warning. The strip has the new card in front. ⌘V in Preview (*Nieuw van klembord* / *New from Clipboard*) pastes it; there, *Extra › Toon inspector* / *Tools › Show Inspector* has no GPS or camera make (no GPS tab). The file in Finder is unchanged: *Toon info* / Get Info still shows its location, and its modification date has not changed.
- [ ] Not offered: a copied picture (⌃⇧⌘4 to the clipboard) and a QR code made with *Tekst ▾* / *Text ▾* › *QR Code* have no *Strip Metadata* chip (it is still in *Image ▾*). Neither does a dropped PDF. On *Tekst* / *Text* (the recognized text of a dropped picture), the chip is greyed out like *Image ▾*. Search (type *metadata*) lists *Strip Metadata* once.
- [ ] Debug build: `log stream` (command above) shows no file name, path, coordinates or location for the drop or the step.

### Checksums of a dropped file

- [ ] Shown: make a fake file (`printf 'fake checksum test\n' > /tmp/fake.txt`) and drop it on the card. The chip row ends with *Controlegetallen* / *Checksums* and *Controleer hash van klembord* / *Check Hash from Clipboard*. *Checksums* shows the file name, then `SHA-256` with 64 hex digits and `MD5` with 32; they are the same as `shasum -a 256 /tmp/fake.txt` and `md5 /tmp/fake.txt` in Terminal, and can be selected. Clicking *Checksums* again goes back to the text. A dropped picture and a dropped PDF have the two chips too, and so does a file too large to show (it shows only a note).
- [ ] Compare, ✓: copy the SHA-256 from Terminal (`shasum -a 256 /tmp/fake.txt | cut -c1-64 | pbcopy`) and click *Check Hash from Clipboard*: a toast *✓ Gelijk aan de SHA-256 op het klembord* / *✓ Matches the SHA-256 on the clipboard*, and the card's checksum view says it on the meta line. Uppercase with spaces (`… | tr a-f A-F | sed 's/..../& /g' | pbcopy`) gives ✓ too, and so does the MD5 (`md5 -q /tmp/fake.txt | pbcopy`: *… MD5 …*).
- [ ] Compare, ✗: change one digit of the copied hash: *✗ Anders dan de hash op het klembord* / *✗ Differs from the hash on the clipboard*. Copy plain text (`hello`): *Geen hash op het klembord* / *No hash on the clipboard*.
- [ ] Large file: `mkfile 2g /tmp/big.bin` and drop it. The card stays usable at once; the checksum view says *Berekenen… N%* / *Calculating… N%*, counting up, until the hashes show. Drop another file while it reads: the new file's checksums come, not the old one's. Delete `/tmp/big.bin` afterwards.
- [ ] Debug build: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows no file name, path, hash or clipboard text for the drop or the compare.

### Convert a dropped picture

- [ ] Menu: drop `testdata/images/exif_rotated_gps.jpg` (fake GPS) on the card and open *Afbeelding ▾* / *Image ▾*: the first row is the submenu *Converteren* / *Convert* with *Naar PNG* / *To PNG*, *Naar JPG (80%)* / *To JPG (80%)*, *Breedte 1920 px* / *1920 px Wide*, *Breedte 1280 px*, *Breedte 800 px*; *Wijzig formaat* / *Resize* follows.
- [ ] To JPG: pick *To JPG (80%)*. Within a second the card shows the new copy, its meta line starts with *JPEG* and ends with *Omgezet naar JPG (80%), zonder metadata* / *Converted to JPG (80%), metadata removed*, with no location warning; the strip has it in front. ⌘V in Preview (*Nieuw van klembord* / *New from Clipboard*) pastes it; *Extra › Toon inspector* / *Tools › Show Inspector* shows no GPS tab and no camera make. The file in Finder is unchanged.
- [ ] To PNG: on a dropped JPEG, *To PNG*: the meta line starts with *PNG* and says *Converted to PNG, metadata removed*; the picture is upright (the EXIF turn is in the pixels).
- [ ] Width: drop a screenshot wider than 1920 px (a full-screen ⌘⇧3 on a Retina display) and pick *1920 px Wide*: the copy is 1920 wide with the height in proportion (meta *Resized to 1920×…, metadata removed*), still a PNG. A wide JPEG photo stays a JPEG. A picture already narrower than the width makes no copy and notes *Al dat formaat of kleiner* / *Already that size or smaller*.
- [ ] Save: *Bewaar* / *Save* on the copy still offers PNG, JPEG and HEIC as before; nothing new was added (the app has the user-selected read-write entitlement already).
- [ ] Debug build: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows no file name, path, size or location for the conversion.

### Drop a CSV or JSON file: the table

Make fake files first:
`printf 'naam;plaats;code\n"Jan, de Vries";"Zei ""hoi""";007\nAnja;Utrecht;012\n' > /tmp/rows.csv`
and `printf '[{"naam": "Jan, de Vries", "n": 3}, {"naam": "Anja", "n": 4}]' > /tmp/rows.json`.

- [ ] CSV: drop `/tmp/rows.csv` on the card: the table window opens at once, titled `rows.csv`, with three columns; `Jan, de Vries` and `Zei "hoi"` are one cell each, `007` stays `007`. Do the same with `tables/small_table.csv` (`,`) and `tables/tsv_table.tsv` (tab): the separator is found by itself. ⌘O (*Kies bestand* / *Choose File*) on a CSV does the same.
- [ ] Copy as JSON: Table ▾ › *Kopieer als JSON* / *Copy as JSON*: a toast *Gekopieerd als JSON* / *Copied as JSON*; ⌘V in TextEdit gives a pretty JSON array with `"naam": "Jan, de Vries"`, `"plaats": "Zei \"hoi\""` and `"code": "007"`, keys in column order. After *Remove duplicate rows* or a sort it copies that version. Close the window: the card shows the file with the chips *Open Table* and *Copy as JSON*; the chip copies the file as it is.
- [ ] JSON: drop `/tmp/rows.json`: the table window opens with columns `naam` and `n`. Table ▾ › *Kopieer als CSV* / *Copy as CSV*: toast *Gekopieerd als CSV* / *Copied as CSV*, and ⌘V gives `naam,n`, `"Jan, de Vries",3`, `Anja,4`. Closed again, the card shows the JSON formatted, with *Open Table* and *Copy as CSV*. The same JSON copied as text from another app stays a plain JSON card, with no table chips.
- [ ] Toolbar buttons: with `/tmp/rows.csv` in the table window, the toolbar shows after *Copy* two symbol buttons, `{}` (tooltip *De tabel als JSON-lijst van objecten op het klembord* / *The table on the clipboard as a JSON array of objects*) and a grid (*Copy as CSV*), then *Save*; in a narrow window they are in » as *Kopieer als JSON* / *Copy as JSON* and *Kopieer als CSV* / *Copy as CSV*. `{}` gives the toast *Gekopieerd als JSON* / *Copied as JSON* and the same JSON as Table ▾'s item; the grid button gives *Gekopieerd als CSV* / *Copied as CSV* and `naam,plaats,code` with commas. With `/tmp/rows.json` both work too. Make `printf '[{"a": {"b": 1}}, {"a": {"b": 2}}]' > /tmp/nested.json` and drop it: if it opens as a table, the grid button shows the toast *Deze tabel past niet in die vorm* / *This table doesn't fit that form* and the clipboard is unchanged. While the window says *Working on the table…* both buttons are dimmed. Table ▾'s item and the card chips are still there.
- [ ] Not a table: a `.json` file with one object (`{"a": 1}`) or a list of numbers, and a `.txt` with CSV text, stay on the card as before (the `.txt` still offers *Open Table*). A JSON array whose objects hold nested objects: *Copy as CSV* shows the toast *Deze tabel past niet in die vorm* / *This table doesn't fit that form*.
- [ ] Privacy: `tables/window_payees_iban.csv` dropped opens the window with *financial* on its meta line; with *Use privacy filter* on, the grid is masked until clicked. Put a made-up token (`ghp_` and 36 letters/digits) in a cell of a fake CSV, drop it and *Copy as JSON*: the JSON is on the clipboard but not in history.
- [ ] Debug build: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows only the column and row counts the table window logged before, never a file name, path or cell.

### Search in the table

Make a fake file first: `printf 'naam;plaats\nJan de Vries;Groningen\nAnja;Utrecht\nÉmile;UTRECHT\n' > /tmp/search.csv`.

- [ ] Drop `/tmp/search.csv`: the toolbar ends with a search field (magnifying glass, placeholder *Zoek in de tabel* / *Search the table*, tooltip with ⌘F). In a narrow window it collapses to the magnifying glass.
- [ ] ⌘F puts the cursor in the field. Type `utrecht`: after a short pause only *Anja* and *Émile* are left (case does not matter, any column counts); the meta line reads *2 van 3 rijen × 2 kolommen* / *2 of 3 rows × 2 columns*, labels after it as before. `émile` finds *Émile*; `xyz` leaves no rows and *0 van 3 rijen* / *0 of 3 rows*.
- [ ] Esc (or the field's ×) clears it: all rows again and *3 rijen × 2 kolommen* / *3 rows × 2 columns*. The column widths do not jump while filtering.
- [ ] A view only: with `utrecht` typed, *Copy*, `{}` *Copy as JSON*, *Copy as CSV* and ⌘S *Save* still give all three rows, and no version strip appears (still one version). Right-click a filtered row › *Copy Row* copies that row (not the first row of the table). Remove duplicate rows or a sort keeps the search and filters the new version.
- [ ] Big table: `(echo 'id;naam;stad'; seq 1 64659 | awk '{print $1";naam"$1";stad"$1%500}') > /tmp/big.csv` and drop it. Typing `stad42` in the field stays responsive (no beachball per key); the meta line shows *1.420 van 64.659 rijen × 3 kolommen* / *1,420 of 64,659 rows × 3 columns* (`stad42` and `stad420`–`stad429`).
- [ ] Privacy: close the window and open the table again: the field is empty. Debug build: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows nothing of the query, the cells or the number of matches.

### Table versions in the strip

Use `/tmp/search.csv` from the step above.

- [ ] Drop `/tmp/search.csv` and open the table window: one version, so there is no strip along the bottom (as the history strip on the card hides with one copy) and the toolbar has no ↶ *v1/1* ↷.
- [ ] *Tabel ▾* › *Verwijder dubbele rijen* / *Remove Duplicate Rows* (or a sort): a strip of small cards appears between the grid and the meta line, and the cards read exactly like the history strip's card for the same CSV: the small grey type mark *csv* on top (no *v1* / *v2*, no *Origineel* / *Original*, no size), then the first lines. The first card shows the copied text's first three lines as the history card does; the new one shows its own first lines (the header and its first rows, comma-separated) and is framed in the accent colour. Same font, padding, fill and frame as the history cards. The grid and sidebar move up to make room, nothing overlaps.
- [ ] Hover a card: the tooltip reads *CSV* on the first card and *CSV · Dubbele rijen verwijderd · 2 rijen × 2 kolommen* (the step, then the size once that version was shown) on the second. A TSV or JSON table shows *tsv* / *{}* and its own type name, as its history card.
- [ ] Click the first card: the original shows, its card is framed, the meta line changes with it; the clipboard is untouched (no toast). Click the second: back. ⌘Z and ⇧⌘Z step too and move the frame; the card's own version capsule follows. Steps made on the card before the window opened show their step's name until that version has been shown once, then its lines.
- [ ] With the original shown, *Copy*, *Copy as CSV* and ⌘S *Save* give the original's rows; a new step from it drops the later cards.
- [ ] Do enough steps that the strip is wider than the window: it scrolls sideways and the framed card scrolls into view. Resize the window: the strip follows its width.
- [ ] VoiceOver reads the strip as *Versies van de tabel* / *Table versions* and each card as *Versie 2 van 2, Dubbele rijen verwijderd* / *Version 2 of 2, …* (the first: *Versie 1 van 2, Origineel*), the shown one selected; it does not read the cells. Switch the language: the labels and tooltips change on the next paint.
- [ ] Privacy: with the privacy filter on, switch apps and back: the cards show only *csv* and dots until the table is revealed, as masked history cards; VoiceOver and the tooltip say *Versie 1 van 2* / *Version 1 of 2* only. Wipe or close: the strip is gone; reopened, it starts over. Debug build: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows nothing of the steps, the cells or the sizes.

### Switching table versions with a large file

Make a fake 5 MB table first (65,000 rows, made-up values): `awk 'BEGIN{print "id,naam,plaats,bedrag,datum,code,status,notitie"; for(i=1;i<=65000;i++) printf "%d,naam%d,plaats%d,%d.%02d,2026-%02d-%02d,C%05d,%s,regel %d met wat tekst\n", i, i%977, i%131, i%5000, i%100, i%12+1, i%28+1, i, (i%3?"open":"dicht"), i}' > /tmp/big.csv`.

- [ ] Drop `/tmp/big.csv` and open the table window. Do a step that keeps two columns (*Kies kolommen…*, or *Verwijder dubbele rijen*): the strip shows two cards.
- [ ] Click the other card: its frame moves at once. When the switch takes longer than about 0.15 s the grid dims and a spinner shows over it (VoiceOver: *Versie laden…* / *Loading version…*); a quick switch shows no spinner and does not flash. While it runs the window still moves, resizes and scrolls, and the grid takes no selection or right-click.
- [ ] When the version is there the grid fades in (about 0.2 s) and the meta line shows its size. Click the cards quickly a few times: the grid ends on the last card clicked, never on an older one, and the frame never jumps back.
- [ ] ⌘Z and ⇧⌘Z do the same (the frame moves first). A step from the card's own version capsule dims the window's grid the same way while it runs.
- [ ] Reduce Motion on (System Settings › Accessibility › Display): the switch has no fade, the version just shows; the spinner still shows on a slow switch.
- [ ] Search waits without stale rows: type `naam9` in the search field, then at once click the other version card (or its ×). Until the search of the new version is drawn, the grid is dimmed under the spinner, takes no clicks and a right-click gives no *Copy Row* menu. When it is drawn, right-click a row › *Copy Row* and paste in TextEdit: it is the row shown, of the version framed, and the meta line says that version's *… van … rijen* / *… of … rows*.
- [ ] Stale searches stop: type `n`, `na`, `naa` quickly, then close the window right away. Activity Monitor › CopyCraft › *Sample Process* a moment later shows no `copycraft-table-search` thread still busy, and the memory drops back within a second or so.
- [ ] Search in the large table, then switch: the search applies to the new version. Scroll to the bottom of the grid: the last row shows with its stripe, nothing is cut off; scroll sideways to the last column.
- [ ] Debug build: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows no cells, sizes or file names while switching. Nothing new is written to disk.

### Removing a table version

Use `/tmp/search.csv` from *Search in the table* and `/tmp/big.csv` from the step above.

- [ ] Drop `/tmp/search.csv`, open the table window and make three versions: *Verwijder dubbele rijen* / *Remove Duplicate Rows*, then *Kies kolommen…* / *Choose Columns…* keeping `naam` only, then a sort on `naam`. The strip shows four cards, the last framed. Its card has a × in the top-right corner, the same grey ✕ in the same place and size as on the history cards; the other cards show theirs only while the pointer is over them, and the first card (the original) never has one.
- [ ] Hover the second card (the duplicates step) and click its ×: that card goes, three are left, and the framed card is still the sorted one, now one place to the left. The grid shows the duplicate row again, still `naam` only and sorted (the later steps are replayed without the removed one; nothing else changes). The meta line and the card's version capsule (*v3/3*) follow. The clipboard is untouched and there is no toast.
- [ ] ⌘Z right after: the step comes back where it was, four cards again, and that version is shown (framed, duplicates gone). Any other action first (a click on a card, a step, ⇧⌘Z) and ⌘Z steps back through the versions as before.
- [ ] Click the × of the version shown (the last card): the card before it is framed and shown. With the second card shown, the × on a later card (hover it) takes only that one away; the grid does not change and does not dim. Recount without dimming: with `/tmp/big.csv` from *Switching table versions with a large file*, make three versions: *Kies kolommen…* / *Choose Columns…* keeping `id`, `naam`, `plaats` (*3 van 8 kolommen behouden* / *Kept 3 of 8 columns*), a sort on `naam`, then *Kies kolommen…* keeping `id`, `naam` (*2 van 3 kolommen behouden* / *Kept 2 of 3 columns*). Show the original (first card) and click the × of the second card: the grid does not dim, shows no spinner and does not change; a moment later the last card's tooltip reads *2 van 8 kolommen behouden* / *Kept 2 of 8 columns*. ⌘Z puts the choice back and shows it (that does dim on a slow switch); the last card reads *2 van 3* / *2 of 3* again a moment after the grid is there.
- [ ] ⌫ in the window (the grid clicked, not the search field) takes out the version shown, as ⌫ on the card takes the marked history card out; with the original shown ⌫ does nothing. In the search field ⌫ edits the search. (If plain ⌫ does nothing on your Mac, try ⌘⌫ and report which works.)
- [ ] Take out every step: when only the original is left the strip hides and the grid moves down, as with one version.
- [ ] VoiceOver: on a version card (not the original) the Actions rotor (VO-⌘-Space) has *Verwijder versie* / *Remove Version*; the × reads the same. Switch the language: both change.
- [ ] Large file: with `/tmp/big.csv` make two steps (*Verwijder dubbele rijen*, then a sort), show the last version and click the × of the first step's card: the frame moves at once; when it takes longer than about 0.15 s the grid dims with the spinner, then the new version fades in (no fade with Reduce Motion), and the window stays responsive meanwhile.
- [ ] Privacy: with the cards masked the × still works; debug build: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows nothing of the steps, the cells or the sizes. Nothing new is written to disk.

### Dropping a large CSV

Use `/tmp/big.csv` (5 MB, 65,000 rows) from *Switching table versions with a large file*; make it with the `awk` line there if it is gone. Run once from a debug build (`cargo run -p copycraft`, where it was slowest) and once as the bundled app.

- [ ] Open the card and drop `/tmp/big.csv` on it. The card answers at once: after about 0.15 s a spinner shows over the well with *Bestand openen…* / *Opening file…* under it (VoiceOver reads the spinner the same). The card still moves and the pointer never turns into the beach ball.
- [ ] The card keeps the spinner until the table is there; no empty *Bezig met de tabel…* / *Working on the table…* window shows first. Then the card goes, the table window opens and its grid fades in (about 0.2 s). With Reduce Motion on (System Settings › Accessibility › Display) the grid just shows, no fade; the spinner still shows.
- [ ] A small CSV (`/tmp/search.csv`): the window opens without the card flashing a spinner or *Opening file…*.
- [ ] ⌘O (or *Kies bestand…* / *Choose File…* in the card's menu) and choose `/tmp/big.csv`: the same, spinner and line on the card, then the window with the fade.
- [ ] Stale results: drop `/tmp/big.csv` and, while the spinner shows, copy some text in another app: the card shows the copy, the spinner and line go and no table window opens later. Drop `/tmp/big.csv` and right after a small text file: the card ends on the text file and the big table never opens. Drop a JSON file while the big CSV opens: only the JSON shows.
- [ ] A dropped text or picture that is not a table: the card shows it as before (*Opening file…* only while a large one is read).
- [ ] Debug build: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows no cells, sizes or file names during the drop. Nothing new is written to disk.

### Closing a large table

Use `/tmp/big.csv` (5 MB, 65,000 rows) from *Switching table versions with a large file*. Run once from a debug build (`cargo run -p copycraft`) and once as the bundled app.

- [ ] Drop `/tmp/big.csv`, wait for the grid, scroll to the middle and close the window with its red button: it is gone at once, no beach ball and no stall of the pointer. Again with ⌘W.
- [ ] Right after the close the card comes back with the file (as before) and answers at once: click it, move it, copy something new.
- [ ] Make two versions first (*Verwijder dubbele rijen* / *Remove Duplicate Rows*, then a sort) and close: just as quick.
- [ ] Open the table again from the card: it opens with its grid (the closed window kept nothing on screen: no old rows flash before the new grid).
- [ ] *Alles wissen* / *Clear All* (or lock the Mac) while the big table shows: the window closes at once as well and the card answers right away.
- [ ] Quit with ⌘Q while the big table shows, and right after closing it: CopyCraft quits without a crash report (Console › Crash Reports, nothing new for CopyCraft).
- [ ] Privacy: the content is still wiped from memory, only off the main thread now. Debug build: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows no cells, sizes or file names while closing. Nothing new is written to disk.

### Copy from the table, then close

Use `/tmp/big.csv` (5 MB, 8 columns) from *Switching table versions with a large file*, and a small fake file: `printf 'naam;plaats;leeftijd\nJan de Vries;Groningen;41\nAnja;Utrecht;35\n' > /tmp/three.csv`.

- [ ] Drop `/tmp/three.csv` on the card; the table window opens. *Kies kolommen…* / *Choose Columns…*: keep two columns (a new version). *Kopieer* / *Copy* in the window's toolbar; the toast says it was copied. Close the window: the card comes back showing that two-column copy (title, meta line with 2 columns, the preview with only those two columns), not the dropped file. The strip has the two-column copy as its newest card; the dropped file is still in the strip after it.
- [ ] ⌘V in TextEdit: the same two columns.
- [ ] The same with *Kopieer als CSV* / *Copy as CSV* and *Kopieer als JSON* / *Copy as JSON* (the card then shows the JSON), and with a cell, row or column from the grid's right-click menu.
- [ ] With `/tmp/big.csv`: keep two columns, *Kopieer* / *Copy*, close. The card shows the two-column copy; its meta line may say *Controleren…* / *Checking…* for a moment, and the strip gets the copy as its newest card once that is done.
- [ ] No copy in the window: closing it brings the card back with the dropped file, as before. With the card open while the window closes after a copy, the card moves from the file to the copy.
- [ ] Debug build: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows no cells, sizes or file names. Nothing new is written to disk.

### Dropping a zip

Make fake test zips in Terminal (nothing real in them):

```sh
mkdir -p /tmp/zt/docs/sub && printf 'hallo\n' > /tmp/zt/docs/a.txt && printf 'wereld\n' > /tmp/zt/docs/sub/b.txt
ditto -c -k --sequesterRsrc --keepParent /tmp/zt/docs /tmp/docs.zip   # as Finder's Compress makes it
(cd /tmp/zt && zip -qr /tmp/loose.zip docs)                              # the same with zip
python3 -c 'import zipfile; z=zipfile.ZipFile("/tmp/slip.zip","w"); z.writestr("ok.txt","ok"); z.writestr("../evil.txt","evil"); z.close()'
(cd /tmp/zt && zip -qe -P test /tmp/locked.zip docs/a.txt)              # password protected
printf 'PK\003\004 not really a zip' > /tmp/broken.zip
python3 -c 'import zipfile; zipfile.ZipFile("/tmp/empty.zip","w").close()'
mkdir -p /tmp/unzip-here
```

`python3` comes with the Xcode command line tools; without them skip the slip and empty zips (the unit tests cover them).

- [ ] Drop `/tmp/docs.zip` on the card. *Bestand openen…* / *Opening file…* shows only if reading takes over 150 ms. The card is titled *Zip-archief* / *Zip Archive*; the meta line says `docs.zip  ·  2 bestanden  ·  2 mappen  ·  0.013 KB uitgepakt` (EN: *files*, *folders*, *unpacked*). The text lists `docs/  (2)`, `docs/a.txt`, `docs/sub/  (1)`, `docs/sub/b.txt`; no `__MACOSX`. The chip row is *Uitpakken* / *Extract* and the two checksum chips; no Text ▾, no table. The zip's names are not added to the history strip.
- [ ] ⌘O and choose `/tmp/loose.zip`: the same listing.
- [ ] *Uitpakken* / *Extract*: a panel *Kies waar de bestanden komen* / *Choose where the files go* that picks folders (with *New Folder*), button *Uitpakken* / *Extract*. Choose `/tmp/unzip-here`. A toast *Uitgepakt in unzip-here* / *Extracted to unzip-here*; `ls -R /tmp/unzip-here` shows `docs/a.txt` and `docs/sub/b.txt` with the right contents. Cancel in the panel does nothing.
- [ ] Never overwrite: extract `/tmp/docs.zip` into `/tmp/unzip-here` again. The rule: when any top-level name of the zip already exists in the chosen folder, everything goes into a new folder named after the zip (`docs`, else `docs 2`, `docs 3`, …); otherwise straight into the chosen folder. Here `docs` exists, so the toast says *Uitgepakt in docs 2* / *Extracted to docs 2*, and `/tmp/unzip-here/docs` is untouched (`cat /tmp/unzip-here/docs/a.txt` still says `hallo`).
- [ ] `/tmp/slip.zip`: the listing shows `../evil.txt` and `ok.txt`, the meta line ends with *onveilige namen* / *unsafe names*. *Uitpakken* shows no panel but the toast *Niet uitgepakt: een naam in dit zip-bestand wijst buiten de map…* / *Not extracted: a name in this zip file points outside the folder…*. Nothing appears in `/tmp` or `/tmp/unzip-here`.
- [ ] `/tmp/locked.zip`: the names show, meta *met wachtwoord* / *password protected*; *Uitpakken* says it is protected with a password. `/tmp/broken.zip`: the card says *Dit zip-bestand is beschadigd* / *This zip file is damaged*. `/tmp/empty.zip`: *Dit zip-bestand is leeg* / *This zip file is empty*.
- [ ] A larger zip: `mkdir -p /tmp/zt/many && for i in $(seq 1 1500); do printf 'x%.0s' $(seq 1 100) > /tmp/zt/many/f$i.txt; done && mkfile -n 300m /tmp/zt/many/big.bin && ditto -c -k --keepParent /tmp/zt/many /tmp/many.zip`. The card lists the first 1000 lines, then *… en nog N* / *… and N more*. *Uitpakken* into a new folder: after 150 ms *Uitpakken…* / *Extracting…* under the wheel; the card stays usable and the toast comes when it is done.
- [ ] Sandbox (signed build): extracting into a folder chosen in the panel works (Desktop, Documents, an external disk). A zip attachment dragged from Mail (a promised file) can be extracted while its card is shown.
- [ ] Quarantine (signed, sandboxed build): download a small zip with Safari (or give a fake one the attribute: `xattr -w com.apple.quarantine '0083;00000000;Safari;' /tmp/docs.zip`), extract it, then `xattr -p com.apple.quarantine /tmp/unzip-here/docs/a.txt` prints a value (set by the sandbox, naming CopyCraft), also for a file from a zip that had no attribute. An extracted script with the `x` bit (`chmod +x /tmp/zt/docs/run.sh` before zipping) is `-rwxr-xr-x` and still quarantined. If no attribute is there, report it: the zip's attribute would then have to be copied.
- [ ] Unsafe zips (fake, made with `python3`):

  ```sh
  mkdir -p /tmp/zt/ln && ln -sf /etc/hosts /tmp/zt/ln/link && (cd /tmp/zt/ln && zip -qy /tmp/link.zip link)   # a symlink
  python3 -c 'import zipfile; z=zipfile.ZipFile("/tmp/case.zip","w"); z.writestr("Readme.txt","a"); z.writestr("README.TXT","b"); z.close()'
  python3 -c 'import zipfile; z=zipfile.ZipFile("/tmp/bidi.zip","w"); z.writestr("invoice\u202etxt.exe","a"); z.writestr("two\nlines.txt","b"); z.writestr("zero\u200bwidth.txt","c"); z.writestr("line\u2028sep.txt","d"); z.writestr("bom\ufeff.txt","e"); z.writestr("tag\U000e0041.txt","f"); z.writestr("a\u200db.txt","g"); z.writestr("\U0001f468\u200d.txt","h"); z.writestr("lone\ufe0f.txt","i"); z.writestr("blank\u3164.txt","j"); z.writestr("flag \U0001f3f4\U000e0067\U000e0062\U000e0065\U000e006e\U000e0067\U000e007f.txt","k"); z.close()'
  python3 -c 'import zipfile; z=zipfile.ZipFile("/tmp/plant.zip","w"); z.writestr("docs/.copycraft-partial",""); z.writestr(".docs.partial/a.txt","a"); z.close()'
  python3 -c 'import zipfile,unicodedata as u; z=zipfile.ZipFile("/tmp/nfd.zip","w"); z.writestr(u.normalize("NFC","caf\u00e9.txt"),"a"); z.writestr(u.normalize("NFD","caf\u00e9.txt"),"b"); z.close()'
  ```

  Each card's meta line ends with *onveilige namen* / *unsafe names*, and *Uitpakken* / *Extract* gives the *Niet uitgepakt…* / *Not extracted…* toast without a panel; nothing is written. `/tmp/link.zip` lists `link`. `/tmp/case.zip` lists both names. `/tmp/bidi.zip` lists `invoice\u{202E}txt.exe` `two\u{000A}lines.txt`, `zero\u{200B}width.txt`, `line\u{2028}sep.txt`, `bom\u{FEFF}.txt`, `tag\u{E0041}.txt`, `a\u{200D}b.txt`, `👨\u{200D}.txt`, `lone\u{FE0F}.txt`, `blank\u{3164}.txt` (a Hangul filler, drawn as a space) and the England flag as `🏴\u{E0067}…\u{E007F}.txt` on one line each, so the name cannot pass for `invoiceexe.txt`. `/tmp/nfd.zip` lists `café.txt` twice (the two Unicode forms of one name). `/tmp/plant.zip` (CopyCraft's work-folder mark, and the hidden work-folder name of earlier versions) is refused too.
- [ ] Emoji in names are fine: `python3 -c 'import zipfile; z=zipfile.ZipFile("/tmp/emoji.zip","w"); z.writestr("\U0001f468\u200d\U0001f4bb notes.txt","a"); z.writestr("\U0001f44d\U0001f3fd.txt","b"); z.writestr("\u2764\ufe0f/\U0001f3f3\ufe0f\u200d\U0001f308.txt","c"); z.writestr("1\ufe0f\u20e3 notities.txt","d"); z.writestr("#\ufe0f\u20e3.txt","e"); z.close()'`. The card lists `👨‍💻 notes.txt`, `👍🏽.txt`, `❤️/🏳️‍🌈.txt`, `1️⃣ notities.txt` and `#️⃣.txt` as emoji (no `\u{…}`), without *onveilige namen* / *unsafe names*; *Uitpakken* / *Extract* writes the five files, and Finder shows the same emoji. Flags made of tag characters (England 🏴󠁧󠁢󠁥󠁮󠁧󠁿, Scotland, Wales) are refused on purpose: tag characters can hide any text in a name, and these three flags are the only use of them (see `/tmp/bidi.zip` above).
- [ ] A zip holding its own work folder: `python3 -c 'import zipfile; z=zipfile.ZipFile("/tmp/docs.zip","w"); z.writestr("docs (onvolledig)/a.txt","a"); z.close()'` and `python3 -c 'import zipfile; z=zipfile.ZipFile("/tmp/report.zip","w"); z.writestr("Rapport (onvolledig)/a.txt","a"); z.writestr("docs (incomplete)/b.txt","b"); z.close()'`. `/tmp/docs.zip`'s meta line ends with *naam van een werkmap* / *name of a work folder*, and *Uitpakken* / *Extract* gives *Niet uitgepakt: een map in dit zip-bestand heeft de naam van de werkmap die CopyCraft bij het uitpakken maakt* / *Not extracted: a folder in this zip file has the name of the work folder CopyCraft makes while extracting* (not *wijst buiten de map* / *points outside the folder*), without a panel. Drop `/tmp/report.zip`: no warning, and it extracts `Rapport (onvolledig)` and `docs (incomplete)` as ordinary folders.
- [ ] Small zip bomb: a zip whose header says 1000 bytes but holds 50 MB of zeros:

  ```sh
  python3 - <<'EOF'
  import struct, zipfile
  z = zipfile.ZipFile("/tmp/bomb.zip", "w", zipfile.ZIP_DEFLATED); z.writestr("zeros.bin", b"\0" * 50_000_000); z.close()
  b = bytearray(open("/tmp/bomb.zip", "rb").read())
  struct.pack_into("<I", b, 22, 1000)                                # local header: unpacked size
  struct.pack_into("<I", b, b.rfind(b"PK\x01\x02") + 24, 1000)      # directory: the same lie
  open("/tmp/bomb.zip", "wb").write(b)
  EOF
  ```

  The card shows the lie (about 1 KB *uitgepakt* / *unpacked*); *Uitpakken* into `/tmp/unzip-here` gives *Dit zip-bestand is beschadigd* / *This zip file is damaged*. `ls -a /tmp/unzip-here` shows no `zeros.bin` and no `bomb (onvolledig)` / `bomb (incomplete)`.
- [ ] Free space: make a small disk with `hdiutil create -size 20m -fs APFS -volname Small /tmp/small.dmg && hdiutil attach /tmp/small.dmg`, then extract `/tmp/many.zip` (300 MB) and choose `/Volumes/Small`: at once the toast *Niet uitgepakt: er is te weinig vrije ruimte op die schijf* / *Not extracted: there is not enough free space on that disk*; `ls -a /Volumes/Small` shows nothing new. `/tmp/docs.zip` into `/Volumes/Small` still works. Afterwards `hdiutil detach /Volumes/Small`.
- [ ] Stop: extract `/tmp/many.zip` into a new folder; when *Uitpakken… (⌘. stopt)* / *Extracting… (⌘. to stop)* shows, press ⌘. (or click *Uitpakken* again): the toast *Uitpakken gestopt; er is niets achtergebleven* / *Extracting stopped; nothing was left behind*, and `ls -a` of that folder shows no `many` and no `many (onvolledig)` / `many (incomplete)`. ⌘. without an extraction running does nothing. The same with *Wis geschiedenis* / *Clear History*, *Wipe* or locking the screen (⌃⌘Q) while it extracts: it stops and nothing is left.
- [ ] Quit while extracting: extract `/tmp/many.zip` into a new folder and, while *Uitpakken…* shows, quit (⌘Q, or status menu › *Quit*): CopyCraft quits within a few seconds and `ls -a` of that folder shows nothing new (no `many`, no `many (onvolledig)` / `many (incomplete)`, nothing hidden). While it extracts, Finder shows the work folder `many (onvolledig)` / `many (incomplete)` (never a half `many`).
- [ ] After a crash: extract `/tmp/many.zip` again and, while it extracts, `killall -9 CopyCraft`: the visible `many (onvolledig)` / `many (incomplete)` stays (with a hidden `.copycraft-partial` file in it). Open CopyCraft (switch the language in Settings first, to check both names) and extract it into the same folder again: `many` appears, the work folder is in the Trash (Finder › Prullenmand / Trash: the whole folder, with its half files; *Zet terug* / *Put Back* returns it) and the toast ends with *Een onvolledige map van een eerdere poging staat in de Prullenmand* / *An incomplete folder from an earlier try is in the Trash*. A folder you make yourself with that name (without the `.copycraft-partial` file) is left alone; ours then goes to `many 2 (onvolledig)` / `many 2 (incomplete)`. Crash once more, put a file of your own (`echo mine > ".../many (incomplete)/mine.txt"`) in the work folder, and extract again: the whole work folder, `mine.txt` included, is in the Trash (nothing deleted) and `many` is made in the folder. On a volume without a Trash (a FAT USB stick, a network share) the work folder stays as it is, the next name (`many 2 (onvolledig)`) is used and the toast says nothing about the Trash. A stopped (⌘.) or failed extraction still removes only its own files from its own work folder, without the Trash.
- [ ] Debug build: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows no zip names, paths, sizes or counts. Nothing is written besides the chosen folder.

### Compare two dropped text files

Make two fake files first: `printf 'alpha\nbeta\ngamma\n' > /tmp/a.txt` and `printf 'alpha\nBETA\ngamma\ndelta\n' > /tmp/b.txt`.

- [ ] Two at once: select both in Finder and drop them together on the card: the card opens *Vergelijk* / *Compare* (the diff) with the first file on the left and the second on the right, `beta`/`BETA` marked as changed and `delta` as added. Swap and the other diff tools work as for two history items.
- [ ] Not stored: neither file shows up in history, the strip or search, and after closing and opening the card nothing of them is left. *Wis geschiedenis* / *Clear History* or locking the screen while the diff is open closes it.
- [ ] Not two text files: drop three text files together, a `.txt` with a `.png`, a `.txt` with a folder, or two pictures at once: no diff opens and a toast says *Vergelijken werkt met twee tekstbestanden* / *Compare works with two text files* (switch the language in Settings to check both). One file dropped alone still opens as before.
- [ ] Privacy: put a made-up token (`ghp_` and 36 letters/digits) in `/tmp/b.txt` and drop both again: the diff is masked as for a secret history item, until revealed.
- [ ] ⌘-click two history tiles afterwards: the diff now shows those two, not the dropped files.
- [ ] Debug build: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows no file name, path or text for the comparison.

### Pictures show in the strip at once

- [ ] Two in a row: copy a screenshot (⌘⇧⌃4) and right after it a second one: within about a second the strip under the card has both tiles (the strip shows from 2 entries). Do the same with ⌘V of two pictures into the open card, and with two picture files dropped onto it: same.
- [ ] Taken out late: copy a screenshot of a text with a made-up token (`ghp_` and 36 letters/digits). Its tile shows at once and goes away after its text is read (a few seconds); it does not come back, not even after closing and opening the card. *Copy* of another tile still works.
- [ ] Still kept out: with *Pause* on, or with an ignored app in front, or with a private copy (1Password), a copied picture gets no tile.
- [ ] Debug build: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows `image admitted at once: checked false` then `image checked: retracted false` (or `true` for the token), never a file name or text.

### Review fixes (Oct 2026)

- [ ] Long step with a key: make a JSON file of over 32 KB with `"aws": "AKIAIOSFODNN7EXAMPLE"` in it (for example `python3 -c 'print("[" + ",".join(["{\"k\": \"plain value\"}"]*2000) + ",{\"aws\": \"AKIAIOSFODNN7EXAMPLE\"}]")' > /tmp/big.json`), drop it on the card and take *Format JSON* (or *Text ▾ › Reverse Lines*): the result is on the clipboard (concealed: a clipboard manager that honours it does not show it) and the card notes it is not in history; the strip and the history menu do not have it. The same file without the key: the result is in history.
- [ ] Stale step result: turn on *Pause*, copy the 40 KB file's text (A), take *Text ▾ › Reverse Lines* and copy another text (B) right away, before the result is back: the clipboard keeps B (paste it in TextEdit), the card shows B, no Copied toast for A's result. Same with a picture: *Pause*, copy picture A, *Image ▾ › Rotate*, and copy picture B at once: B stays.
- [ ] Wipe and lock forget the steps: take *Text ▾ › Reverse Lines* on a copy, then *Wipe*: ⌘Z on the card does nothing and no step note is left. Take a step again and lock the screen (⌃⌘Q) right after, also with history empty before the copy (*Clear History* first): after unlocking, the card shows no file, no browsed copy and no ⌘Z; an AI run that was open is gone.
- [ ] Steps without names: *Text ▾ › Filter…* with a pattern, then ⌘Z and ⇧⌘Z: back and forth as before. On a picture, *Image ▾ › Rotate 90° Left*: the meta line still says *Rotated 90° left*.
- [ ] Log without kinds: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'`, then copy `testdata/sensitivity/credential_github_token.txt`'s text, take *Text ▾ › Check AGENTS.md* on an AGENTS.md with a fake token and *Complexity* on a file over its limit: the lines say `copy skipped: Refused`, `agent check: N warnings` and `complexity: too large (N bytes)` (a scanner that fails: `complexity: scanner failed (N bytes)`), never *Credential*, *secrets* or a language.
- [ ] Back to an earlier text: copy `alpha`, take *Text ▾ › UPPERCASE* and then *Text ▾ › lowercase* (A, B, A): history has `alpha` once, in front, and `ALPHA`; ⌘Z on the card goes to `ALPHA`, ⌘Z again to `alpha`, ⇧⌘Z forward again. Same on a picture: *Rotate 90° Left* four times round, then ⌘Z steps back each turn.
- [ ] Whitespace-only result: copy a few lines with empty lines between them (`a`, empty, `b`), take *Text ▾ › Filter…* with pattern `.` and *invert* on: a step makes no copy of only empty lines, so the clipboard keeps the copy and the card never says *Copied, not in history (too large)*. (Should one ever get through, the note is *Copied, not in history (empty)* / *Gekopieerd, niet in de geschiedenis (leeg)*.)
- [ ] A picture step not kept: turn on *Pause*, drop a PNG on the card and take *Image ▾ › Grayscale*: the card leaves the dropped file and shows the grey copy (the clipboard's) with *Copied, not in history (recording paused)* on its meta line. Same with a copied picture under *Pause*: the note shows on the grey copy. With *Pause* off and a picture copied while CopyCraft did not record it yet, the step puts both in the status menu's history at once.

### Screenshots (fase 1)

- [ ] First time: reset with `tccutil reset ScreenCapture nl.marcelkoopman.copycraft` and `defaults delete nl.marcelkoopman.copycraft CopycraftScreenshotAsked`, then press ⌃⌥4: CopyCraft explains why (*Doorgaan* / *Continue*), then macOS asks. Allow it, quit and open CopyCraft again if macOS says so.
- [ ] Area: ⌃⌥4, drag a rectangle: a thumbnail shows bottom right for about 5 s; paste in Preview (*New from Clipboard*): exactly that area, sharp on a Retina screen. The strip under the card has the new tile; it has no versions.
- [ ] Whole screen and window: ⌃⌥4 and a click (no drag) gives the screen under the pointer; ⌃⌥4, Space and a click on a Finder window gives only that window. Esc or a right-click cancels: nothing changes on the clipboard.
- [ ] Menu and search: the menu bar menu has *Take Screenshot…* / *Maak schermafbeelding…* with ⌃⌥4 beside it; the card's command search finds *Take Screenshot…* too.
- [ ] Thumbnail: a click on it opens the card on the screenshot; dragging it into TextEdit or Mail drops the PNG; after the drop or the click it goes away. With the pointer on it, it stays until the pointer leaves.
- [ ] Denied: turn CopyCraft off in System Settings › Privacy & Security › Screen & System Audio Recording, press ⌃⌥4: a dialog with *Open Privacy & Security…* opens that pane.
- [ ] Denied outside the .app: started with `cargo run` (a bare binary, no bundle), macOS gives Screen Recording to the app that started it (Zed, Terminal), so CopyCraft is never in that list. Press ⌃⌥4 without that permission: the dialog says *Screen Recording is off for the app that started CopyCraft* / *Schermopname staat uit voor de app die CopyCraft startte* and tells you to turn on that app (such as Zed or Terminal) and restart it, or open the bundled CopyCraft.app; *Open Privacy & Security…* still opens the pane. From the .app it is the *… off for CopyCraft* text above.
- [ ] Tip for an unsigned (ad-hoc) CopyCraft.app: after a rebuild macOS no longer matches the old entry, so the switch looks on while capture is refused. Select CopyCraft in Screen & System Audio Recording, remove it with the minus button (−), add the new CopyCraft.app again with plus (+) and reopen CopyCraft.
- [ ] Own windows: open the card, take a whole-screen screenshot: the card is not on it. Tick Settings › Privacy › *Show CopyCraft in screenshots* and again: the card is on it (Settings itself stays out).
- [ ] Pause: turn on *Pause Recording*, take one: it pastes, but the strip gets no new tile.
- [ ] Wipe and lock: take a screenshot and press *Wipe* (or lock the screen) while the thumbnail shows: the thumbnail goes and the tile is gone from the strip.
- [ ] Hotkey: Settings › General › *Screenshot* › *Change…*, press ⌃⌥5: ⌃⌥5 now takes screenshots and ⌃⌥4 does not; trying the card hotkey (⌃⌥Space) there says *Already used by the other hotkey — try again*. Put ⌃⌥4 back.
- [ ] Log: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows only `screenshot: captured` (or `screenshot: failed (not allowed)` and the like): no sizes, no app or window names.

### Preview button waits

- [ ] Quit Preview first. Copy a picture, open the card, hover the right edge of the picture and click *Voorvertoning* / *Preview* (the arrows button): until Preview's window is up the button shows a small spinner and is dimmed; a second click does nothing. Then the button is back, and the next click opens Preview again (quick now Preview runs).
- [ ] VoiceOver on: while it spins the button reads *Voorvertoning laden…* / *Loading preview…*.
- [ ] Close the card (Esc) right after the click: Preview still opens; open the card again: the button is normal.
- [ ] Click and press *Wipe* (or lock the screen) at once: the spinner goes, and no Preview window shows the picture (or it is gone after Wipe).

### Default hotkeys ⌃⌥Spatie / ⌃⌥4

- [ ] Fresh defaults: `defaults delete nl.marcelkoopman.copycraft CopycraftHotkey` and `defaults delete nl.marcelkoopman.copycraft CopycraftScreenshotHotkey` (an error that the key does not exist is fine), open CopyCraft. ⌃⌥Space (Control-Option-Space) opens and closes the card in any app; ⌃⌥4 starts a screenshot. Settings › General shows *⌃⌥Spatie* and *⌃⌥4* (English: *⌃⌥Space*); the welcome screen says *… met ⌃⌥Spatie.* / *… with ⌃⌥Space.*, ✕'s tooltip *Sluit (Esc, ⌃⌥Spatie)* / *Close (Esc, ⌃⌥Space)*, and the status menu's first row ⌃⌥Spatie / ⌃⌥Space.
- [ ] Every keyboard: the same defaults on an ISO keyboard, a US (ANSI) keyboard and a JIS keyboard (whichever are at hand, or switch the input source to U.S. and to Japanese – Kana): ⌃⌥Space and ⌃⌥4 work on each, nothing to set. Plug in or switch keyboards while CopyCraft runs: the hotkeys keep working and Settings does not change.
- [ ] Status menu and search: *Take Screenshot…* / *Maak schermafbeelding…* has ⌃⌥4 beside it; the card's command search finds *Take Screenshot…* with ⌃⌥4.
- [ ] Input menu: add a second input source (System Settings › Keyboard › Text Input › Edit…, e.g. U.S.) and check System Settings › Keyboard › Keyboard Shortcuts › Input Sources: when *Select next source in Input menu* is on with ⌃⌥Space, note which one wins when you press ⌃⌥Space (the card, or the input source switching), and whether Settings › General says *Sneltoets in gebruik, kies een andere* / *Hotkey in use, pick another* under the hotkey. Hovering the hotkey in Settings shows the tooltip about *Select next source in Input menu*. Turn that shortcut off (or record another chord): ⌃⌥Space opens the card.
- [ ] Taken by another app: in another app that can set a global shortcut (e.g. Raycast, Alfred, or a second copy of CopyCraft from another build), take ⌃⌥4 first, then open CopyCraft: Settings › General says *Sneltoets in gebruik, kies een andere* / *Hotkey in use, pick another* under *Schermafbeelding* / *Screenshot*; the menu item still takes a screenshot and CopyCraft starts normally (also when ⌃⌥Space is the taken one: the card opens from the menu bar). Record another chord: the note goes back to the normal text and the new chord works.
- [ ] Recorded chords stay: in Settings record ⌃⌘C for the card and ⌃⇧4 for screenshots, quit and open CopyCraft: both still work and ⌃⌥Space / ⌃⌥4 do nothing. The same with the old defaults recorded (⌥§ and ⌥⇧§; the menu draws ⌥⇧§ beside *Take Screenshot…*). Delete the two keys again (step 1) for the new defaults.
- [ ] Rules: recording ⌥C says *Need ⌃ or ⌘ (⌥ or ⌥⇧ alone only with §) — try again*; recording ⌥` on a US keyboard says the same (⌥` is a dead key: ⌥` then e still types è); ⌃⌥Space for the screenshot while the card has ⌃⌥Space says *Already used by the other hotkey — try again*.
- [ ] Log: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` at launch shows `hotkey registered` (or a register failure with an error kind only): no keys, no chords.

### Screenshot review fixes (Oct 2026)

- [ ] Text first: open `testdata/sensitivity/credential_github_token.txt` in TextEdit (large font), take an area screenshot of the token: the clipboard gets it (paste in Preview), but the strip gets no new tile. Take one of a line with an e-mail address: it is in the strip, masked/concealed at once (not only after a moment), and a clipboard manager that honours `org.nspasteboard.ConcealedType` skips it. A screenshot without text: a normal tile.
- [ ] Log: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` while taking one shows `screenshot checked: text in true, concealed false, admitted true` (or the like) and `screenshot: captured`: flags only, no text, no window names.
- [ ] Settings itself stays out: open Settings and the card, tick Settings › Privacy › *Show CopyCraft in screenshots*, take a whole-screen screenshot: the card is on it, Settings is not (macOS 15 and later too). Open About and the welcome screen (status menu): not on it either. Untick it: the card is gone too.
- [ ] Overlay and thumbnail stay out: take a whole-screen screenshot, and right after (thumbnail still showing) another: the second shows neither the dimmed overlay nor the first thumbnail. Space and a click on Settings' window picks nothing there (Settings is not offered).
- [ ] ⌥⇧ with a letter: in Settings record ⌥⇧O for the card: *Need ⌃ or ⌘ (⌥ or ⌥⇧ alone only with §) — try again* (⌥⇧O types Ø). Recording ⌥⇧§ for screenshots is still accepted; record ⌃⌥4 again afterwards (or delete the key, see *Default hotkeys ⌃⌥Spatie / ⌃⌥4*).
- [ ] Back to the app: click in TextEdit, take a screenshot with the hotkey (an area): press ⌘V at once without clicking anything: it pastes in TextEdit. Again but cancel with Esc: TextEdit is in front (its menu bar shows). Started from the status menu while the card was open: the card stays in front.
- [ ] Overlay goes: start a screenshot, then ⌘Tab to another app: the overlay is gone, nothing is taken. Start one and plug in or unplug a display (or change the resolution in System Settings › Displays): the overlay is gone, no stuck dimmed screen.
- [ ] Thumbnail goes with a token: take a screenshot of `testdata/sensitivity/credential_github_token.txt` with its text read in time: no thumbnail at all. Take one whose text is not read within about 1 s (the whole screen with a lot of text, a busy Mac): when the full check finds the token the thumbnail closes at once, the tile leaves the strip. A screenshot without a token: the thumbnail stays its 5 s.
- [ ] Screen Recording explanation (reset it: `tccutil reset ScreenCapture nl.marcelkoopman.copycraft`): it says *nooit naar een bestand, behalve als je hem zelf ergens heen sleept* / *never to a file unless you drag it somewhere yourself*.
- [ ] Fewer copies: take ten whole-screen screenshots on a 5K or 4K display, one after the other: in Activity Monitor CopyCraft's memory settles back (each adds about one PNG to history, not several), and with Pause on it settles back to where it was. The thumbnail is sharp (Retina) and has the screenshot's shape; a tiny area screenshot shows at its own size; dragging it to Finder or Mail still gives the full-size PNG.
- [ ] Thumbnail for VoiceOver: take a screenshot and rest the pointer on the thumbnail: tooltip *Schermafbeelding — klik om te openen, sleep om te delen* (English: *Screenshot — click to open, drag to share*). With VoiceOver on (⌘F5), move to it within 5 s (VO-arrows): it reads that label as a button; VO-Space opens the card on it.
- [ ] Unavailable (only on a system without ScreenCaptureKit, otherwise skip): the screenshot hotkey shows the alert *Schermafbeeldingen werken niet op deze Mac* / *Screenshots don't work on this Mac* instead of doing nothing.

## Linkkaart

Mac only, fake links. Nothing is fetched: the card is built from the copied text alone. Check in light and dark mode (also with the *Navy* / `#07112D` theme) that every line and label is readable.

- [ ] Link card: copy `https://www.github.com/example/app/pull/42?tab=files&utm_source=test` and open the card. On top a globe and *github.com* (no `www.`) in large type, then *GitHub · example/app · PR #42*, no path line (the site line says it), the chips *Tabblad* / *Tab* and *utm_source* (names only, no values) and the grey label *Tracking*. The chips are *Openen* / *Open*, *Kopieer opgeschoond* / *Copy cleaned*, *Kopieer alleen domein* / *Copy domain only* and *URL*.
- [ ] Copy buttons: *Kopieer opgeschoond* puts `https://www.github.com/example/app/pull/42?tab=files` on the clipboard (the toast says it is copied); *Kopieer alleen domein* puts `github.com`. Both are in the history strip.
- [ ] Known sites: `https://gitlab.com/group/project/-/merge_requests/7`, `https://example.atlassian.net/browse/ABC-123` (*Jira · ABC-123*), `https://www.notion.so/Plan-0123456789abcdef0123456789abcdef`, `https://docs.google.com/document/d/abc/edit`, `https://www.figma.com/design/abc/Mockup`, `https://stackoverflow.com/questions/123/test` and `https://www.youtube.com/watch?v=aBcDeFgHiJk` (*YouTube · Video*) each show their site line. `http://localhost:3000/api` shows the network symbol, *localhost · poort 3000* / *localhost · port 3000* and the grey label *http*.
- [ ] Readable names, no raw path or IDs: `https://www.youtube.com/watch?v=aBcDeFgHiJk&t=42s&list=PLfake` shows *YouTube · Video · aBcDeFgHiJk*, no `/watch` line and the chips *Starttijd* / *Start time* and *Afspeellijst* / *Playlist* (no `v`, `t` or `list`, and not `42s` or `PLfake`: the video ID is the only value shown, and only on YouTube). `https://youtu.be/aBcDeFgHiJk` shows *YouTube · Video · aBcDeFgHiJk*; `https://www.youtube.com/shorts/aBcDeFgHiJk` *YouTube · Short · aBcDeFgHiJk* with the chip *Short*. A `v=` that is not 11 letters, digits, `_` or `-` (`?v=tooshort`) is not shown: the chip *Video-ID* / *Video ID* instead. In the history strip a YouTube tile shows *youtube.com*, *YouTube · Video* and the ID on a third line; two different videos are told apart by it. With the privacy filter on the tile still shows only the symbol and dots. VoiceOver on the card reads the ID too. `https://docs.google.com/document/d/abc/edit` shows *Document-ID* / *Document ID* and no path; the Figma link *Bestand-ID* / *File ID* (and *Geselecteerd onderdeel* / *Selected layer* for `?node-id=1-2`); the Stack Overflow link *Vraagnummer* / *Question number*; the Notion link *Pagina-ID* / *Page ID*; the GitLab and Jira links no path line. An unknown parameter keeps its raw name: `…/watch?v=aBcDeFgHiJk&feature=share` adds the chip *feature*. *Decodeer* / *Decode* still shows the full URL with its values.
- [ ] Facebook and Instagram: `https://www.facebook.com/reel/1092344577040057` shows *facebook.com*, *Facebook · Reel · 1092344577040057* and no path line; on its strip tile the number is the third line, under *Facebook · Reel*. The same shape with `/watch?v=…` gives *Facebook · Video · <number>*, `/someone/videos/<number>` too (*someone* appears nowhere), `https://fb.watch/aBcDeF1234/` *Facebook · Video · aBcDeF1234*. Only a video's ID shows, and only digits (8 to 20) or for fb.watch letters and digits: `/reel/10923a4577040057` shows *Facebook · Reel* without an ID. `/someone/posts/…` gives *Facebook · Bericht* / *Post*, `/groups/…` *Groep* / *Group*, `/events/…` *Evenement* / *Event*, `/photo/?fbid=…` *Afbeelding* / *Image* (no `fbid` chip), all without the number or user. Instagram shows no ID. `https://www.instagram.com/reel/aBcDeF1234/` shows *Instagram · Reel*, `/p/…` *Instagram · Bericht* / *Post*. In the history strip these tiles show the site line under the domain.
- [ ] TikTok: `https://www.tiktok.com/@someone/video/7312345678901234567` shows *tiktok.com*, *TikTok · Video · 7312345678901234567* and no path line; *@someone* appears nowhere on the card, its strip tile or in VoiceOver, and the tile shows the number as its third line, so two TikTok tiles differ. `/@someone/photo/<number>` gives *TikTok · Afbeelding* / *Image* with the number. Only 15 to 20 digits show: `/video/12345` shows no ID. `https://vm.tiktok.com/ZMabc123/` gives *TikTok · Video · ZMabc123* (a code with `-` or `_` is hidden), `https://www.tiktok.com/@someone` *TikTok · Profiel* / *Profile* with the path line, `/tag/cats` *TikTok · Hashtag*, and `https://www.tiktok.com/nl-NL/` or the bare root just *TikTok*. Another path such as `/some/other/page` shows the plain path, no site line. A profile or hashtag shows no ID. None of these links gets a *Financieel* / *Financial* or *PII* label, and with the privacy filter on their tiles are masked (globe and dots) as before.
- [ ] Site symbols: a YouTube link (`youtube.com`, `youtu.be`, `m.youtube.com`) shows a red play rectangle (`play.rectangle.fill`, #FF0000) instead of the globe, a Facebook link (incl. `fb.watch`) a blue *f* square (`f.square.fill`, #1877F2), a TikTok link a pink music note (`music.note`, #FE2C55), on the big Link card and on its strip tile; Instagram, GitHub and other sites keep the globe, `localhost` the network symbol. These are system symbols, no logos. The colours stay the same in light, dark and every card theme. With the privacy filter on a masked YouTube, Facebook or TikTok tile shows the plain globe and dots, not the site's symbol.
- [ ] Long path and many parameters: a URL with a path of 100 characters is cut in the middle (`…`); one with 7 query parameters shows 4 chips and *+3*.
- [ ] Token: copy `https://example.com/callback?code=fakecode1234567890&state=x`. The orange label *⚠︎ Bevat mogelijk token* / *⚠︎ May contain a token* is shown. *Openen* first asks *Link met mogelijk een token openen?* / *Open a link that may contain a token?*: *Annuleer* / *Cancel* (and Esc) opens nothing, *Openen* / *Open* opens the browser.
- [ ] Credentials: copy `https://user:geheim@example.com/report`. The label *Bevat gebruikersnaam/wachtwoord* / *Contains user name/password* is shown, `geheim` appears nowhere on the card, and *Openen* opens `https://example.com/report` without it.
- [ ] Punycode: copy `https://bücher.example/` and `https://xn--bcher-kva.example/`. Both show the domain as `xn--bcher-kva.example` with the orange label *Internationale domeinnaam: controleer de schrijfwijze* / *International domain name: check the spelling*. A look-alike domain (a Cyrillic `а` in `pаypal.example`) shows as `xn--pypal-4ve.example` with the same label.
- [ ] File link: copy `file:///Users/test/rapport.pdf`. The card shows the doc symbol, *Bestand op deze Mac* / *File on this Mac*, the path and the grey label *file*, with no *Openen* and no copy buttons.
- [ ] Link in text: copy `Kijk even op https://example.com/docs voor de uitleg`. It stays a text card, with the extra chip *Link openen* / *Open link*, which opens `https://example.com/docs`. Two links in one text, or an XML namespace URL, give no *Link openen*.
- [ ] VoiceOver: on a link card VoiceOver reads *Link naar github.com, GitHub · example/app · PR #42* (EN *Link to …*); on a link without a known site *Link naar example.com, webpagina* / *Link to example.com, web page*, never the whole URL. With the card blurred it reads only *Inhoud van het klembord* / *Clipboard content*.
- [ ] Privacy: after all of the above, look at the log (Console, process CopyCraft) and at *Exporteer diagnose…* / *Export Diagnostics…*: no URL, domain, parameter or site name appears.

## Crashlogs en labels (Mac only, no data)

- [ ] Crash log: open any report in Console › *Crashrapporten* / *Crash Reports* (or `~/Library/Logs/DiagnosticReports/*.ips`) and copy all of it (the *Translated Report* text, or the `.ips` file's text). The card is titled *Crashlog* / *Crash log*, is a text card (no table, no YAML or JSON) and has no *PII* or *financieel* / *financial* label, even with `User ID: 501`, process ids, addresses (`0x…`) and timestamps in it.
- [ ] Log: in Terminal run `log show --last 1m --style syslog | head -50 | pbcopy` and open the card: it is titled *Log*, with no *PII* or *financieel* / *financial* label.
- [ ] A token still counts in a log: add a line with a fake GitHub token (`ghp_` and 36 letters and digits) to the copied crash log or log: the card shows *inloggegevens* / *credential* and the copy stays out of history.
- [ ] No card number in a link: copy `https://www.facebook.com/reel/1092344577040057`: the Link card has no *financieel* / *financial* label. The same for a YouTube link with a long number in it and for `0x4111111111111111`.
- [ ] Real numbers keep their label: copy `4111 1111 1111 1111` (a test card): *financieel* / *financial*. Copy `NL91ABNA0417164300` (a test IBAN): *financieel* / *financial*; change one digit: no label. Copy `bel +31 6 12345678`: *PII*.
- [ ] BSN only with its name: copy `123456782` (a fake number that passes the elfproef): no label. Copy `BSN: 123456782`: *PII*. The same with *burgerservicenummer*, *sofinummer*, *sofi-nummer* or *citizen service number* before it on the same line (any case, at most about 40 characters before the number). With the word on the line above, or after the number (`123456782 is mijn BSN`), there is no label. A pid or order number with 8 or 9 digits (`klantnummer 123456782`) has none either.
- [ ] Links in the strip: copy a YouTube link, a Facebook link and a text, then open the card so the strip shows. The link cards show the globe symbol (in the mark colour, as on the big Link card; YouTube, Facebook and TikTok their own coloured symbol, see *Site symbols*) with the domain next to it (`youtube.com`, no `www.`) and the site or path below; no `://` and no `https://www.youtube…`. `http://localhost:3000` shows the network symbol and `localhost:3000`. With the privacy filter on the link card shows the globe and dots. VoiceOver reads *Link*. Check light, dark and a fixed card theme.
- [ ] No inner frame on a link tile: show a link copy so its strip tile is the selected one, then click it and step with ←/→ (also with Full Keyboard Access on): the tile has only the outer accent frame, like a text tile; no second blue rectangle around the globe and the text. The globe and text sit centred in the tile, the text clear of the left and right edges.

## Quit without a crash (Mac only, no data)

Run each step twice: once from Terminal or Ghostty with `cargo run -p copycraft` (unbundled, a crash shows there), once as the bundled `CopyCraft.app`. After each quit, check that no *CopyCraft quit unexpectedly* report appears and that Console › *Crashrapporten* / *Crash Reports* has no new copycraft entry.

- [ ] Right after launch: start CopyCraft, open the menu bar icon's menu within 5 s and choose *Stop* / *Quit*. It quits at once.
- [ ] After the card: open the card (hotkey), close it with Esc, then quit from the menu bar icon. Again with the card still open.
- [ ] After Settings, About and Welcome (Welcome via `defaults delete nl.marcelkoopman.copycraft hasSeenWelcome` and a restart): with each one open, and after closing it by its red button, quit from the menu bar icon.
- [ ] ⌘Q: with the card or a CopyCraft window in front, press ⌘Q (the app menu's *Stop CopyCraft* / *Quit CopyCraft*): it quits the same way, without a crash report. Also from Activity Monitor (*Stop* / *Quit*, not *Force Quit*) and the table window (*Open table* on a CSV) open.
- [ ] Closing is still safe: open and close Settings, About and Welcome five times each (red button, ⌘W), wait 30 s: CopyCraft keeps running and each opens again normally.

## ⌘K: the card's actions (Mac only, fake data)

- [ ] Copy `{"naam": "Voorbeeld"}` and open the card. ⌘K: the search field says *Acties voor deze kaart* / *Actions for this card* and the row shows *Kopieer*, *Bewaar…  ⌘S* and the card's actions (Format, Minify, convert, Tekst… rows); no history rows, *Thema*, *Pauzeer opname* or *Maak schermafbeelding…*.
- [ ] Type `min`: only the matching actions stay. ↓ / ↑ move the accent, Return runs it, and the list closes. Esc (also with typed text) closes the list; ⌘K again closes it too. Typing on the card afterwards opens the normal search (*Zoeken* / *Search*).
- [ ] On a picture, a link, a CSV and a colour (`#336699`) ⌘K lists only that card's actions (Image ▾ rows, link parts, table rows, colour tools). On an empty clipboard ⌘K beeps.
- [ ] On a CSV, sort a column (one table step): ⌘K shows *Maak tabelstap ongedaan  ⌘Z*; after ⌘Z the redo row with *⇧⌘Z*.
- [ ] VoiceOver: ⌘K says *N acties voor deze kaart, typ om te filteren* / *N actions for this card, type to filter*; the field is read as *Acties voor deze kaart*, each row as a button with its title. In English the same in English.
- [ ] Privacy: the log (Console, process CopyCraft) shows no typed text, action names or content.

## Search by kind (Mac only, fake data)

- [ ] Turn history on, then copy `https://example.com/docs`, `{"naam": "Voorbeeld"}`, `a,b` + newline + `1,2`, `#336699`, `boodschappen voor morgen` (pin it) and take a screenshot. Open the card and press `/`: the row shows *Alles*, *Tekst*, *Link*, *Code*, *Tabel*, *Afbeelding*, *Kleur*, *Vastgepind* (EN *All*, … *Pinned*).
- [ ] Click *Link*: the field says `soort:link ` (EN `type:link `) and only the link copy shows, after *Alles*. Type `docs`: it stays; type `zzz`: only *Alles*. Click *Alles*: the prefix goes, the typed text stays.
- [ ] Type `type:afbeelding` and `soort:image`: only the screenshot. `soort:kleur`: only `#336699`. `type:pinned morgen`: only the pinned text. `soort:onbekend` searches the text as typed.
- [ ] Choose *Code*, close the card with the hotkey (not Esc), open it and type `n`: the field says `soort:code n` (check that the caret is at the end). Esc, open again and type `n`: no prefix.
- [ ] With history off (or empty) `/` shows the card's chips as before, not the kind chips.
- [ ] A kind no copy has is dimmed. VoiceOver reads each chip by its name.
- [ ] Privacy: quit and start CopyCraft: no kind is remembered (`defaults read nl.marcelkoopman.copycraft` has none). The log shows no search text or kinds.

## Picture previews after the typed ImageIO port (Mac only, fake data)

Use made-up pictures: a large PNG and JPEG (e.g. 6000×4000), a HEIC (Preview › Export › HEIC) and a JPEG with EXIF orientation 6 (set it with `exiftool -Orientation=6 -n file.jpg`, or use an iPhone portrait image of a plain wall).

- [ ] Copy each file's contents (open in Preview, ⌘A ⌘C) and also copy each file in Finder (⌘C): the card preview shows the picture upright and sharp, no larger than before, and the size line shows the full size (for the rotated one: height × width swapped, e.g. 3024 × 4032 portrait).
- [ ] The history table and the strip show the same thumbnails, upright. Memory in Activity Monitor stays about where it was before (no jump of hundreds of MB on the large PNG).
- [ ] Take a screenshot with the hotkey: its card preview and thumbnail appear as before.
- [ ] Image ▾ on each picture (rotate, resize, grayscale, export PNG / JPEG / HEIC): the exported files look and measure the same as with the previous build. A PDF page preview (copy a PDF file) still renders.
- [ ] Copy a text file in Finder and a broken file renamed to `.png`: no preview, no crash.

## Image ▾ export after the shared ImageIO helpers (Mac only, fake data)

- [ ] With the previous build, export one made-up picture (a PNG with transparency, a JPEG with EXIF orientation 6, a HEIC) via Image ▾ as PNG, as JPEG at 80 % and as HEIC, rotated once. Do the same with this build: the files have the same pixel size, look the same (transparent corners white in the JPEG, the rotated picture upright before the turn), and are about the same file size (`ls -l`).
- [ ] The card says *Bevat locatie* / *Contains location* only for a made-up picture with a GPS block (`exiftool -GPSLatitude=52 -GPSLatitudeRef=N file.jpg`), not for one without; the exported file has no location (`mdls -name kMDItemLatitude`).
- [ ] Card previews of the same pictures (pasted and copied in Finder) are unchanged; a renamed text file gives no preview and no crash.

## About, Settings and Welcome with the shared window delegate (Mac only, no data)

Run once from Terminal (`cargo run -p copycraft`, a crash shows there) and once as the bundled app.

- [ ] About (`⋯` › About): open, close with the red button; open, close with ⌘W; open, press Esc (nothing or close, as in the previous build). Open it five times in a row this way: no crash, it opens each time. Its buttons (suggest a feature, report a problem, privacy, support) still work.
- [ ] Settings: the same with the red button and ⌘W. Type a list in the symbols or ignored apps field and close the window without pressing Return: after opening again the list is kept. Start recording a hotkey, then close: recording has stopped (the next keys do nothing in Settings).
- [ ] Welcome (`defaults delete nl.marcelkoopman.copycraft hasSeenWelcome`, restart): close with the red button, ⌘W and Esc: it does not come back after a restart (marked seen). *Aan de slag* / *Get started* closes it the same way.
- [ ] With the card open, open each window above the card, then close it: the card keeps its level and stays usable.
- [ ] Change the language in Settings with About and Welcome open: they close and open again in the new language, without a crash; close them afterwards.
- [ ] Quit while open: with About, Settings and Welcome each open (and all three together), quit from the menu bar icon and with ⌘Q: no *CopyCraft quit unexpectedly* report and no new entry in Console › Crash Reports.

## Settings kept after a restart, through mac_ui::defaults (Mac only, fake data)

- [ ] In Settings change: card theme, language (NL ↔ EN), the card hotkey, the screenshot hotkey, the symbol list (type `→ ✓ €`, close), and add a made-up ignored app. Insert a symbol from the card once. Quit and start CopyCraft: every value is as you left it, and `defaults read nl.marcelkoopman.copycraft` shows them (no clipboard content among them).
- [ ] The app appearance (System / Light / Dark, where Settings offers it) is also kept after a restart.
- [ ] A CSV card's table preview looks as before (all columns, no `…` column, the card's own row limit).

## Fewer unsafe blocks in mac-ui: text, pasteboard, buttons, blur (Mac only, fake data)

- [ ] Card text: copy made-up text with a long line and a URL; the card shows it with the same fonts, colours and wrapping as the previous build. Search in the card for a word that occurs three times: all three are highlighted, the current one stands out (its own text colour), ↑/↓ move it; clearing the search removes every highlight.
- [ ] Drop onto the card: a made-up file from Finder, two files at once, a message from Mail and a picture from Photos (file promises): each arrives as before; a folder and a web link behave as in the previous build.
- [ ] Paste into the card: plain text, a PNG and a TIFF (copied in Preview) and a file copied in Finder (file URL): each shows the same card and preview as before.
- [ ] Buttons: the checkboxes in the column picker of a CSV card toggle columns; the push buttons on the card and in Settings still respond.
- [ ] The blur behind the card looks the same in light and dark mode (Reduce transparency on: no blur, as before).

## Blur and QR code through objc2-core-image (Mac only, fake data)

- [ ] Turn on *Vervagen* / *Blur* in Settings and copy a made-up secret (e.g. `testwachtwoord-123`) and a made-up picture: the card's text and picture are blurred as strongly as in the previous build; Reveal removes the blur, masking again brings it back. With blur off the opaque shade shows instead, as before.
- [ ] A CSV card with a masked column: the table window and the frozen column are blurred the same way.
- [ ] Make a QR code of made-up text (`https://example.com/test`) from the card: the picture looks as before and the iPhone camera reads the same text. Very long text (over 3000 characters) gives no QR code and no crash.

## Screenshots through objc2-screen-capture-kit (Mac only, made-up screen content)

Put only made-up content on screen (e.g. a TextEdit window with `testscherm`).

- [ ] Area, window and whole-screen screenshot (menu bar icon and the screenshot hotkey): each thumbnail shows the right part at full sharpness (Retina: twice the points), without the pointer, the selection overlay or an earlier thumbnail; CopyCraft's own windows are left out unless included in Settings.
- [ ] A window screenshot of a left-out window is refused as before; one on a second display, if there is one, works.
- [ ] Screen Recording off (System Settings › Privacy): the same notice as before, no crash; after allowing it, the next screenshot works.
- [ ] No file appears (`ls -t ~/Desktop ~/Pictures ~/Library/Containers/nl.marcelkoopman.copycraft/Data/tmp | head`): the picture stays in memory.

## Open at Login through objc2-service-management (Mac only, no data)

Use the bundled, signed app from /Applications (SMAppService needs a real bundle).

- [ ] Settings › *Openen bij inloggen* / *Open at Login*: turn it on: CopyCraft appears in System Settings › General › Login Items (or System Settings opens there when approval is needed); the checkbox stays on after closing and reopening Settings. Log out and in: CopyCraft starts.
- [ ] Turn it off: it leaves Login Items, the checkbox stays off; after a restart it does not start at login.
- [ ] Run once from Terminal (`cargo run -p copycraft`, not a bundle): Settings opens without a crash; switching the option shows the same message as the previous build (unavailable or an error text), not an empty or English-only placeholder.
- [ ] The Welcome window's Open at Login choice behaves the same.
- [ ] (Split unsafe blocks in the capture, 2026-10-30) Repeat the area, window and whole-screen screenshot from "Screenshots through objc2-screen-capture-kit" once more, on a Retina and a non-Retina display if there is one: same size and sharpness, own windows and the pointer still left out.

## Error texts through thiserror (Mac only, fake data)

The error enums derive `thiserror::Error`; the texts the user sees must be exactly as before.

- [ ] A table step on a CSV card with 20 versions already: the same "max versions" note (NL and EN). A table step on made-up text that is no table: the same "not a table" text.
- [ ] An Image ▾ step that changes nothing (e.g. rotate 0, or strip metadata from a picture without any): the same note as before.
- [ ] Drop a made-up password protected `.docx` and a made-up broken `.zip`: the same NL/EN notes as before, no file name in Console (`log stream --predicate 'subsystem == "nl.marcelkoopman.copycraft"'`).
- [ ] JSON ▾ restyle on made-up broken JSON (`{"test": 1,}`): the same "line, column" note; with a duplicate key the same duplicate-key note.
- [ ] Feedback › copy the mail text, and copy a symbol from the symbol window: both still land on the clipboard; Help › mail with no mail app set up shows the same message as before.
- [ ] (mac-ui and mac-ai, same change) The menu bar icon looks as before; Open… (file panel) and Save… still work and Cancel changes nothing; an AI step with Apple Intelligence off shows the same "unavailable" message as before.

## Chart in the table window (Mac only, fake data)

Build on the Mac (`cargo build -p copycraft`): the build log must not show `copycraft: swiftc not found` (then the Chart button is hidden on purpose). Copy this made-up table and open it in the table window (Open table):

```
category,month,revenue,units
Books,2026-01-05,10.5,1
Games,2026-01-20,20,2
Books,2026-02-03,4.5,3
Toys,2026-02-14,7,1
Games,2026-02-28,30,4
Books,2026-03-01,5,2
Toys,2026-03-09,,2
```

- [ ] The toolbar shows a chart symbol (*Grafiek* / *Chart* as its tooltip and in the overflow menu) right after *Tabel ▾* / *Table ▾*. Click it: a bar chart of the sum of revenue per category (Games 50, Books 20, Toys 7), largest first, legend at the bottom, numbers in the language's format (NL `20,5`-style decimals); the note reads *1 waarde overgeslagen* / *1 value skipped*.
- [ ] Pop-ups above the chart: type (bar, line, scatter), the column on the x-axis, the date step (only for the month column), the calculation (count, sum, average, smallest, largest) and the value column (hidden for count). Each change redraws; month gives Jan–Mar, year gives 2026; scatter plots revenue against units without grouping.
- [ ] Esc, and the *Tabel* / *Table* button, go back to the grid; the Chart button again brings the chart back. Esc in the search field still clears the search there.
- [ ] VoiceOver (⌘F5): each bar or point reads its label and value; the pop-ups read their names (*Soort grafiek*, *Kolom op de x-as*, … / *Chart Type*, *X-Axis Column*, …).
- [ ] Switch Settings › Language while the chart is up: the pop-ups, labels and legend follow.
- [ ] Click another app: the window masks; the chart is hidden under the cover and the pop-ups are off; clicking reveals it again.
- [ ] A step (e.g. remove a column) or another version in the strip: the chart follows; a version with no columns that fit goes back to the grid. Close the window and open it again: the grid shows, not the chart.
- [ ] Not tied to CSV: copy the same table as TSV (tabs, e.g. pasted from Numbers or Excel), as a JSON table (`[{"category":"Books","revenue":10.5},{"category":"Games","revenue":20}]`) and with `;` separators: Open table shows the Chart button each time and the chart gives the same sums. A drop of a made-up `.tsv` or `.json` file does too.
- [ ] A table with 60 categories (`printf 'c,v\n'; for i in $(seq 1 60); do echo "k$i,$i"; done`, copied): at most 50 bars, the last *Overig* / *Other*.
- [ ] Nothing in Console (`log stream --predicate 'process == "CopyCraft"'`) with a column name, value or chart label; no new file in `~/Library/Containers/nl.marcelkoopman.copycraft/Data` (`ls -lt … | head`).
- [ ] `codesign -d --entitlements - /Applications/CopyCraft.app` shows the same entitlements as the previous build; `otool -L` on the binary lists only system libraries and `/usr/lib/swift/…`.
