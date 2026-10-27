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
| `detect/link.txt` | Link | plain text | Link | — | Visit | kind=Url; link=https://example.com/docs/getting-started; visit=https://example.com/docs/getting-started |
| `detect/youtube_link.txt` | YouTube link | plain text | YouTube | — | Visit | kind=Url; visit=https://www.youtube.com/watch?v=aBcDeFgHiJk |
| `detect/link_userinfo.txt` | Link with `user:pass@` (Visit strips it; the URL view leaves it out) | plain text | Link | credential | Visit, URL | kind=Url; visit=https://example.com/private/report?id=7; decode~id = 7; decodetitle=URL; decodemeta~credential |
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
| `decode/url_query.txt` | URL chip on a link: scheme, host, path and each query parameter decoded (`%xx`, `+`) | plain text | Link | — | Visit, URL | decode~Path: /zoek/fietsen & zo; decode~q = rode fiets; decode~city = Den Haag; decode~Fragment: top; decodetitle=URL |
| `decode/url_encoded_whole.txt` | URL chip: a URL encoded as a whole, decoded and split | plain text | Content | — | Text ▾, URL | decode~q = copy craft |
| `decode/url_negative.txt` | No chip: a link without query or escapes, `100% done`, prose, a `mailto:` (PII; each line checked in `decode/url.rs`) | plain text | Content | PII | Text ▾ | kind=Text |
| `convert/json_to_avro.json` | JSON → Avro schema | json | Valid JSON | — | Text ▾, Schema | schema~"type": "record"; schema~"name": "order_id" |
| `convert/xml_to_xsd.xml` | XML → XSD schema | xml | Valid XML | — | Text ▾, Schema | schema~<xs:element name="order"> |
| `convert/xsd_to_sample.xsd` | XSD → sample XML | xml | Valid XML | — | Text ▾, Sample | sample~<customer>sample</customer> |
| `tables/header_on_line_3.csv` | Table: title and Definition line above the header | plain text | CSV | — | Open Table | meta~Header on line 3; read=4x4; notes=Header on line 3 |
| `tables/dates_ambiguous.csv` | Table: dates that fit dd/mm and mm/dd | plain text | CSV | — | Open Table | read=5x2; notes=Dates read as dd/mm/yyyy; dtypes=date,i64; dataframe~2026-02-01; dfmeta~Dates read as dd/mm/yyyy |
| `tables/dates_day_over_12.csv` | Table: a day over 12 in the second part forces mm/dd/yyyy | plain text | CSV | — | Open Table | read=5x2; notes=Dates read as mm/dd/yyyy; dtypes=date,i64; dataframe~2026-01-13; dfcard~2026-01-13; dfmeta~Dates read as mm/dd/yyyy |
| `tables/dates_mixed_orders.csv` | Table: one date only fits dd/mm, another only mm/dd (stays text) | plain text | CSV | — | Open Table | read=3x2; notes=none; dtypes=str,i64; dataframe~13/01/2026 |
| `tables/dates_iso.csv` | Table: ISO dates and datetimes | plain text | CSV | — | Open Table | read=4x3; notes=none; dtypes=date,datetime[μs],f64 |
| `tables/semicolon_decimal_comma.csv` | Table: `;` and decimal comma, numbers typed as the table is read | plain text | CSV | — | Open Table | read=4x4; notes=Dates read as dd/mm/yyyy; dtypes=date,str,f64,i64; dfcard~24.95 |
| `tables/leading_zero_ids.csv` | Table: IDs with a leading zero (`007`) stay text when read | plain text | CSV | — | Open Table | read=3x2; dtypes=str,str; dfcard~007 |
| `tables/messy_table.csv` | Table: duplicate rows, empty rows and column (and a constant column, kept) | plain text | CSV | — | Open Table | read=8x5; step:Dedupe=5x5; step:DropEmpty=6x4; step:DropEmpty>Dedupe=4x4 |
| `tables/wide_prefix_20_columns.csv` | Table: 20 columns sharing a name prefix | plain text | CSV | — | Open Table | read=6x20; columns~… PV1 Generation; dfcard~┌ |
| `tables/join_orders.csv` | Table: orders (once the Join with example; now a plain table) | plain text | CSV | — | Open Table | read=4x3 |
| `tables/datetime_first_column.csv` | Table: datetimes like `2026-09-01 08:15` in the first column (the colon in the time is no `key: value`) | plain text | CSV | — | Open Table | kind=Csv; read=4x3; dtypes=datetime[μs],str,f64; dataframe~2026-09-01 08:15; dataframe~Greenhouse south |
| `tables/ledger_filter.csv` | Table: a datetime first column and negative amounts (once the Filter example) | plain text | CSV | financial | Open Table | read=5x3; dtypes=datetime[μs],str,f64 |
| `tables/small_table.csv` | Table: a small table | plain text | CSV | — | Open Table | read=4x3 |
| `tables/wide_six_columns.csv` | Table: six columns with long values, wider than the card (the first column stays in view while scrolling sideways, in the card and in Open Table) | plain text | CSV | — | Open Table | read=4x6; frozen~│ Office chair ┆; dfcard~Trading Company; windowmeta~4 rows × 6 columns; window~Trading Company |
| `tables/window_payees_iban.csv` | Table: Open Table on a sensitive table (IBANs): the window opens revealed and names the labels | plain text | CSV | financial | Open Table | read=3x4; windowmeta~3 rows × 4 columns; windowmeta~financial; window~Bob Jansen; windowcolumns=payee, iban, amount, paid on |
| `tables/sales_by_region.csv` | Table: text, whole numbers, decimals and dates, dates kept as dates | plain text | CSV | — | Open Table | read=6x5; dtypes=str,str,i64,f64,date |
| `tables/tsv_table.tsv` | Table: TSV (tab separated) | plain text | TSV | — | Open Table | kind=Tsv; read=3x4; dataframe~Desk lamp |
| `sensitivity/credential_private_key.txt` | Credential: private-key header (fake body) | plain text | Content | credential | Text ▾ | kind=Text |
| `sensitivity/credential_aws_key.txt` | Credential: AWS example key id | plain text | Content | credential | Text ▾ | kind=Plain |
| `sensitivity/credential_github_token.txt` | Credential: GitHub token (synthetic, as in `corpus.rs`) | plain text | Content | credential | Text ▾ | kind=Plain |
| `sensitivity/credential_stripe_key.txt` | Credential: Stripe-style example key | plain text | Content | credential | Text ▾ | kind=Plain |
| `sensitivity/pii_email.txt` | PII: `example.com` email | plain text | Content | PII | Text ▾ | kind=Plain |
| `sensitivity/pii_phone.txt` | PII: fake 06 number | plain text | Content | PII | Text ▾ | kind=Plain |
| `sensitivity/pii_bsn.txt` | PII: test BSN 111222333 | plain text | Content | PII | Text ▾ | kind=Plain |
| `sensitivity/pii_names_column.csv` | PII: a names column (`naam`) | plain text | CSV | PII | Open Table | kind=Csv |
| `sensitivity/financial_iban.txt` | Financial: test IBAN | plain text | Content | financial | Text ▾ | kind=Plain |
| `sensitivity/financial_card.txt` | Financial: test card 4111 1111 1111 1111 | plain text | Content | financial | Text ▾ | kind=Plain |
| `sensitivity/financial_salary_column.csv` | Financial: a salary column (`salaris`) | plain text | CSV | financial | Open Table | kind=Csv |
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
| `visit=U` | Visit opens `U` (userinfo stripped, as `menubar::visit_current`) |
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
- [ ] Icon tooltips: hover each icon-only button. ⋯ says *Meer* / *More*, ✕ *Sluit (Esc, ⌥§)* / *Close (Esc, ⌥§)* (the hotkey set in Settings); the well's buttons say *Kopieer …*, *Bewaar … (⌘S)* / *Save … (⌘S)*, *Verberg* / *Hide*, *Toon regelnummers* / *Show Line Numbers* and *Open … in Voorvertoning*. Switch the language in Settings: the tooltips follow at once, the same words VoiceOver reads.
- [ ] Short header: open `detect/java_missing_brace.java`. The header says only *Java* (no ⌥§ after it); the meta line starts with *missing }*, then lines and size. A valid Rust file says *Rust*. Change the hotkey in Settings: ✕'s tooltip shows the new chord.
- [ ] Window titles: open Settings: the title bar says *Algemeen* / *General*; ⌘2 makes it *Privacy*, ⌘3 *Weergave* / *Display*. Copy the text of `tables/dates_iso.csv` and *Open Table*: the window is titled *Tabel* / *Table*. Choose `tables/dates_iso.csv` with ⌘O and *Open Table*: the title is `dates_iso.csv`. Welcome and About keep their titles.
- [ ] Chip symbols: a JSON card's *Tekst ▾* / *Text ▾* chip shows the `textformat` glyph (Aa), *Origineel* / *Original* a curved back arrow (not a photo), a Base64 copy's decoder chip `</>` (not an open lock). `⋯` shows *Pauzeer opname* with a plain pause glyph and *Over CopyCraft* with the app symbol (a rounded square, not the *i* of *Info*). Add two copies to the diff: *Wissel A en B* / *Swap A and B* is in `⋯` (two swapped rectangles), not a chip, also when the card has four chips. No item shows an empty gap where a symbol failed to load.
- [ ] Table window toolbar: open a CSV from `tables/` with *Open tabel*. The window has a native toolbar in its title bar (unified compact: one row with the title, no separate glass strip above the sidebar): *Tabel ⌄* as text with its menu (the same steps as before, a pick works, *Kies kolommen…* opens its dialog after the menu closes), then *Kopieer* and *Bewaar* as symbol buttons only (two pages, a tray with a down arrow), and at the right ↶ *v1/1* ↷ as one group; *v1/1* opens the version list, the one shown checked. Hovering shows *Kopieer de getoonde tabel*, *Bewaar de getoonde tabel*, *Maak tabelstap ongedaan (⌘Z)* style tooltips; VoiceOver reads the names. Right-click the toolbar: no *Customize Toolbar…*, no icon/text choice. Do a step (Remove Duplicate Rows): ↶ enables, the title reads *v2/2*. Make the window narrow: items that do not fit go under » with their names. Copy and Save are dimmed while *Working on the table…*. With the privacy filter on, switch apps and back: *Klik op de tabel om hem te tonen* shows in the toolbar between the buttons and the versions. Switch the language to English: titles and tooltips turn English. Wipe: the window closes; reopened, the menus hold the new table only.
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
- [ ] ⌘V in the search field (type `/` on the card) and in the column picker's filter pastes
      the clipboard text into the field; ⌘V with search closed still pastes onto the card.
- [ ] Hidden Edit menu: in Settings › AI's prompt fields and the card's search field, ⌘X ⌘C ⌘V
      ⌘A and ⌘Z/⇧⌘Z work on the text; on the card without a field, ⌘Z still steps back
      through the card's versions. ⌘H hides CopyCraft and ⌘Q quits it (winit's app menu).
- [ ] Way back: hide the menu bar icon (System Settings › Menu Bar, or ⌘-drag it off), then
      double-click CopyCraft in Finder (or open it from Launchpad): the card opens in the middle
      of the screen. The welcome screen's last line says *Menu bar icon hidden? Open CopyCraft
      again from the Finder or with ⌥§.* (Dutch: *Is het menubalkpictogram verborgen? …*).
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
- [ ] In the window: Remove Duplicate Rows, then ↶/↷ and ⌘Z/⇧⌘Z and the version menu; the
      card follows. Resize, scroll sideways (Product stays visible), ⌘M minimises, ⌘W closes.
- [ ] Reopen the window, then Wipe, lock (⌃⌘Q), sleep and Clear History: each closes it.
      Copying new items until its entry leaves history also closes it.
- [ ] `tables/window_payees_iban.csv` in the window: the meta line shows financial in red.
- [ ] Open Table on any table opens the window; its Table ▾ shows exactly: Remove duplicate
      rows, Remove Empty Rows and Columns, Choose Columns…, Sort Ascending ›, Sort Descending ›. The
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
- [ ] Paste access denied (macOS 15.4+ with the pasteboard privacy preview on): set CopyCraft to *Weiger* / *Deny* under System Settings › *Privacy en beveiliging* / *Privacy & Security* › *Plakken vanuit andere apps* / *Paste from Other Apps*, copy something and open the card. Under its lines the card has the link *Open Privacy en beveiliging…* / *Open Privacy & Security…*, at the bottom of the well and not over the text (also on a Compact card). Click it: System Settings opens on *Paste from Other Apps* (or, if macOS does not know that anchor, on *Privacy & Security*). The empty card still has *Open privacy-instellingen…* / *Open Privacy Settings…*, which opens CopyCraft's own Settings › Privacy. Copy the link `x-apple.systempreferences:com.apple.preference.security` and choose Visit (if offered) or click it in a card: it is still not opened.
- [ ] History wording: Settings › *Privacy*: under *Klembordgeschiedenis bijhouden* the note reads, on two lines and not cut off, *Leest elke kopie, alleen in het geheugen; weg bij stoppen of vergrendelen. macOS vraagt één keer of CopyCraft andere apps mag lezen.* (English: *Reads every copy, in memory only; gone when you quit or lock. macOS asks once whether CopyCraft may read other apps.*); nothing below overlaps it and the tab is a little taller than *Algemeen* (switching animates the height). The welcome window (quit, `defaults delete nl.marcelkoopman.copycraft hasSeenWelcome`, start again) says the same under its history checkbox, with the retention time (5, 15 or 60 min) and *Anders alleen wat je in de kaart plakt of neerzet.*
- [ ] Privacy policy: open `docs/privacy.md` (or the page after the next Pages build): the date is 23 October 2026; both languages have the retention time (5/15/60 min, *Tot wissen* / *Until Cleared*), *Gevoelige kopieën na 60 s wissen* / *Clear sensitive copies after 60 s*, *Meldingen* / *Notifications* (local, never your text) and, under *Wat wel lokaal wordt opgeslagen* / *What is stored locally*, the temporary folder for a file dragged from Photos or Mail, deleted after reading. The Dutch history paragraph now names the ⌘Z copy like the English one. Drag a picture from Photos onto the card, then look in Finder at `~/Library/Containers/nl.marcelkoopman.copycraft/Data/tmp/` (or `$TMPDIR` without the sandbox): no `mac-ui-drop-…` folder is left once the card shows it.
- [ ] Apple Intelligence note: on a Mac with Apple Intelligence on, copy a paragraph of text and click the **AI** chip: the last line, dimmed and not clickable, reads *Draait op je Mac met Apple Intelligence; er gaat geen tekst naar een server.* (EN: *Runs on your Mac with Apple Intelligence; no text is sent to a server.*). Searching the card for "server" does not offer it. With Apple Intelligence off, the menu shows only the dimmed reason line as before.

## Drops and Image ▾ steps (Mac only, no data)

- [ ] Drop keeps the copied text: copy a line of text in TextEdit, open the card right away (before anything else), then drag a picture file from Finder onto it: the card shows the picture, and the strip has both the picture (at the front, at once) and the copied text behind it. Click the text card: it is there. Esc: the card shows the clipboard's text again. Repeat with a long text (a few pages, still being checked when you drop) and with *Pauzeer* / *Pause* on: paused, the text is not added (as before). A fake token (`ghp_` and 36 letters and digits) copied before the drop stays out of history, as before.
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

- [ ] First time: reset with `tccutil reset ScreenCapture nl.marcelkoopman.copycraft` and `defaults delete nl.marcelkoopman.copycraft CopycraftScreenshotAsked`, then press ⌥⇧§: CopyCraft explains why (*Doorgaan* / *Continue*), then macOS asks. Allow it, quit and open CopyCraft again if macOS says so.
- [ ] Area: ⌥⇧§, drag a rectangle: a thumbnail shows bottom right for about 5 s; paste in Preview (*New from Clipboard*): exactly that area, sharp on a Retina screen. The strip under the card has the new tile; it has no versions.
- [ ] Whole screen and window: ⌥⇧§ and a click (no drag) gives the screen under the pointer; ⌥⇧§, Space and a click on a Finder window gives only that window. Esc or a right-click cancels: nothing changes on the clipboard.
- [ ] Menu and search: the menu bar menu has *Take Screenshot…* / *Maak schermafbeelding…* with ⌥⇧§ beside it; the card's command search finds *Take Screenshot…* too.
- [ ] Thumbnail: a click on it opens the card on the screenshot; dragging it into TextEdit or Mail drops the PNG; after the drop or the click it goes away. With the pointer on it, it stays until the pointer leaves.
- [ ] Denied: turn CopyCraft off in System Settings › Privacy & Security › Screen & System Audio Recording, press ⌥⇧§: a dialog with *Open Privacy & Security…* opens that pane.
- [ ] Own windows: open the card, take a whole-screen screenshot: the card is not on it. Tick Settings › Privacy › *Show CopyCraft in screenshots* and again: the card is on it (Settings itself stays out).
- [ ] Pause: turn on *Pause Recording*, take one: it pastes, but the strip gets no new tile.
- [ ] Wipe and lock: take a screenshot and press *Wipe* (or lock the screen) while the thumbnail shows: the thumbnail goes and the tile is gone from the strip.
- [ ] Hotkey: Settings › General › *Screenshot* › *Change…*, press ⌃⌥5: ⌃⌥5 now takes screenshots and ⌥⇧§ does not; trying the card hotkey (⌥§) there says *Already used by the other hotkey — try again*. Put ⌥⇧§ back.
- [ ] Log: `log stream --level debug --predicate 'subsystem == "nl.marcelkoopman.copycraft"'` shows only `screenshot: captured` (or `screenshot: failed (not allowed)` and the like): no sizes, no app or window names.

### Preview button waits

- [ ] Quit Preview first. Copy a picture, open the card, hover the right edge of the picture and click *Voorvertoning* / *Preview* (the arrows button): until Preview's window is up the button shows a small spinner and is dimmed; a second click does nothing. Then the button is back, and the next click opens Preview again (quick now Preview runs).
- [ ] VoiceOver on: while it spins the button reads *Voorvertoning laden…* / *Loading preview…*.
- [ ] Close the card (Esc) right after the click: Preview still opens; open the card again: the button is normal.
- [ ] Click and press *Wipe* (or lock the screen) at once: the spinner goes, and no Preview window shows the picture (or it is gone after Wipe).

### Default hotkeys ⌥§ and ⌥⇧§

- [ ] Fresh defaults: `defaults delete nl.marcelkoopman.copycraft CopycraftHotkey` and `defaults delete nl.marcelkoopman.copycraft CopycraftScreenshotHotkey` (an error that the key does not exist is fine), open CopyCraft. ⌥§ (Option and the key left of 1) opens and closes the card in any app; ⌥⇧§ starts a screenshot. Settings › General shows *⌥§* and *⌥⇧§*; the welcome screen says *… with ⌥§.*, ✕'s tooltip *Close (Esc, ⌥§)*, and the status menu's first row ⌥§.
- [ ] Status menu: *Take Screenshot…* / *Maak schermafbeelding…* has ⌥⇧§ drawn beside it (the § set by hand; if it is missing or shows another glyph, say so).
- [ ] Recorded chords stay: in Settings record ⌃⌘C for the card and ⌃⇧4 for screenshots, quit and open CopyCraft: both still work and ⌥§ / ⌥⇧§ do nothing. Delete the two keys again (step 1) for the new defaults.
- [ ] Rules: recording ⌥C says *Need ⌃ or ⌘ (⌥ or ⌥⇧ alone only with § or `) — try again*; ⌥§ for the screenshot while the card has ⌥§ says *Already used by the other hotkey — try again*.
- [ ] US keyboard (if one is at hand, or an ANSI MacBook): with the keys deleted, the defaults are ⌥` and ⌥⇧` (the key left of 1 there), shown with ` in Settings and the menu.

### Screenshot review fixes (Oct 2026)

- [ ] ⌥⇧ with a letter: in Settings record ⌥⇧O for the card: *Need ⌃ or ⌘ (⌥ or ⌥⇧ alone only with § or `) — try again* (⌥⇧O types Ø). ⌥⇧§ for screenshots is still accepted.
