use super::{CurlTarget, FormValue, carries_credential, convert, looks_like_curl, parse};

const POST: &str = "curl -X POST https://api.example.com/items \\\n  -H 'Content-Type: application/json' \\\n  -H \"X-Trace: a b\" \\\n  --data-raw '{\"name\": \"lamp\", \"n\": 2, \"ok\": true, \"tag\": null}'";

#[test]
fn only_a_curl_command_is_one() {
    assert!(looks_like_curl("curl https://example.com"));
    assert!(looks_like_curl("  $ curl -s https://example.com"));
    assert!(!looks_like_curl("curly braces"));
    assert!(!looks_like_curl("use curl to fetch it"));
    assert_eq!(parse("curl -s"), None);
}

#[test]
fn options_and_continued_lines_are_read() {
    let request = parse(POST).unwrap();
    assert_eq!(request.method, "POST");
    assert_eq!(request.url, "https://api.example.com/items");
    assert_eq!(
        request.headers,
        [
            ("Content-Type".to_string(), "application/json".to_string()),
            ("X-Trace".to_string(), "a b".to_string())
        ]
    );
    assert!(request.body.as_deref().unwrap().contains("\"lamp\""));
    // `-d` without a method is a POST form; two `-d` join with `&`.
    let request = parse("curl -d a=1 -d b=2 example.com/x").unwrap();
    assert_eq!(request.method, "POST");
    assert_eq!(request.body.as_deref(), Some("a=1&b=2"));
    assert!(request.headers.contains(&(
        "Content-Type".into(),
        "application/x-www-form-urlencoded".into()
    )));
    // `-G` puts the data in the query.
    let request = parse("curl -G https://e.com/s?x=1 --data-urlencode q=lamp").unwrap();
    assert_eq!(request.method, "GET");
    assert_eq!(request.url, "https://e.com/s?x=1&q=lamp");
    assert_eq!(request.body, None);
    // `--url`, `-XPUT`, `--json`, skipped options with a value.
    let request =
        parse("curl -sSL -o out.json -XPUT --url https://e.com --json '{\"a\":1}'").unwrap();
    assert_eq!(request.method, "PUT");
    assert_eq!(request.url, "https://e.com");
    assert!(request.json);
    assert!(
        request
            .headers
            .contains(&("Accept".into(), "application/json".into()))
    );
    // `-u`, `-F` with a file, `--header=`.
    let request =
        parse("curl -u demo:secret -F name=lamp -F 'file=@/tmp/a b.png;type=image/png' --header='X: 1' e.com").unwrap();
    assert_eq!(request.user, Some(("demo".into(), Some("secret".into()))));
    assert_eq!(
        request.form,
        [
            ("name".into(), FormValue::Text("lamp".into())),
            ("file".into(), FormValue::File("/tmp/a b.png".into()))
        ]
    );
    assert_eq!(request.method, "POST");
    assert!(request.headers.contains(&("X".into(), "1".into())));
    // `$'…'` and a non-ASCII short option do not trip the reader.
    assert_eq!(
        parse("curl -H $'X-A: 1\\t2' e.com").unwrap().headers[0].1,
        "1\t2"
    );
    assert!(parse("curl -é e.com").is_some());
}

#[test]
fn fetch() {
    let out = convert(POST, CurlTarget::Fetch).unwrap();
    assert!(
        out.starts_with("const response = await fetch(\"https://api.example.com/items\", {\n"),
        "{out}"
    );
    assert!(out.contains("  method: \"POST\",\n"), "{out}");
    assert!(out.contains("    \"X-Trace\": \"a b\",\n"), "{out}");
    assert!(
        out.contains("  body: JSON.stringify({\n    \"name\": \"lamp\","),
        "{out}"
    );
    let out = convert("curl -u demo:pw -F a=1 e.com", CurlTarget::Fetch).unwrap();
    assert!(out.contains("form.append(\"a\", \"1\");"), "{out}");
    assert!(out.contains("btoa(\"demo:pw\")"), "{out}");
    assert!(out.contains("body: form"), "{out}");
}

#[test]
fn python() {
    let out = convert(POST, CurlTarget::Python).unwrap();
    assert!(out.starts_with("import requests\n\n"), "{out}");
    // `json=` sets the Content-Type itself.
    assert!(!out.contains("Content-Type"), "{out}");
    assert!(out.contains("\"ok\": True,"), "{out}");
    assert!(out.contains("\"tag\": None,"), "{out}");
    assert!(out.contains(
        "response = requests.post(\"https://api.example.com/items\", headers=headers, json=json_data)"
    ), "{out}");
    let out = convert(
        "curl -X PURGE -u demo:pw -F f=@a.txt e.com",
        CurlTarget::Python,
    )
    .unwrap();
    assert!(
        out.contains("files = {\n    \"f\": open(\"a.txt\", \"rb\"),\n}"),
        "{out}"
    );
    assert!(
        out.contains(
            "requests.request(\"PURGE\", \"e.com\", files=files, auth=(\"demo\", \"pw\"))"
        ),
        "{out}"
    );
}

#[test]
fn httpie() {
    let out = convert(POST, CurlTarget::Httpie).unwrap();
    assert!(out.starts_with("http --raw='{\"name\""), "{out}");
    assert!(
        out.contains(
            " POST https://api.example.com/items Content-Type:application/json 'X-Trace:a b'"
        ),
        "{out}"
    );
    let out = convert(
        "curl -F 'n=it'\"'\"'s' -F f=@a.txt e.com",
        CurlTarget::Httpie,
    )
    .unwrap();
    assert_eq!(out, "http --form POST e.com 'n=it'\\''s' f@a.txt\n");
}

#[test]
fn reqwest() {
    let out = convert(POST, CurlTarget::Reqwest).unwrap();
    assert!(
        out.contains("    .post(\"https://api.example.com/items\")\n"),
        "{out}"
    );
    assert!(out.contains("    .header(\"X-Trace\", \"a b\")\n"), "{out}");
    assert!(out.contains("    .body(\"{\\\"name\\\""), "{out}");
    assert!(out.ends_with("    .send()\n    .await?;\n"), "{out}");
    let out = convert("curl -X PURGE -u demo e.com", CurlTarget::Reqwest).unwrap();
    assert!(
        out.contains(".request(reqwest::Method::from_bytes(b\"PURGE\")?, \"e.com\")"),
        "{out}"
    );
    assert!(out.contains(".basic_auth(\"demo\", None::<&str>)"), "{out}");
}

#[test]
fn swift() {
    let out = convert(POST, CurlTarget::Swift).unwrap();
    assert!(out.starts_with("var request = URLRequest(url: URL(string: \"https://api.example.com/items\")!)\nrequest.httpMethod = \"POST\"\n"), "{out}");
    assert!(
        out.contains("request.setValue(\"a b\", forHTTPHeaderField: \"X-Trace\")"),
        "{out}"
    );
    assert!(
        out.contains("request.httpBody = Data(\"{\\\"name\\\""),
        "{out}"
    );
    // A `\(` in the data is no interpolation.
    let out = convert("curl -d 'a\\(b)' e.com", CurlTarget::Swift).unwrap();
    assert!(out.contains("Data(\"a\\\\(b)\".utf8)"), "{out}");
    let out = convert("curl -F a=1 -F f=@/x/y.png e.com", CurlTarget::Swift).unwrap();
    assert!(out.contains("let boundary = UUID().uuidString"), "{out}");
    assert!(out.contains("filename=\\\"y.png\\\""), "{out}");
}

#[test]
fn credentials_are_known() {
    assert!(carries_credential(
        "curl -H 'authorization: Bearer x' e.com"
    ));
    assert!(carries_credential("curl -H 'Cookie: a=b' e.com"));
    assert!(carries_credential("curl -b a=b e.com"));
    assert!(carries_credential("curl -u me e.com"));
    assert!(!carries_credential("curl -H 'Accept: */*' e.com"));
    assert!(!carries_credential("not curl"));
}

#[test]
fn a_curl_copy_offers_the_five_conversions() {
    use crate::text_ops::code_style::{CodeLang, CodeOp};
    let lang = CodeLang::of(POST, crate::format::detect(POST));
    assert_eq!(lang, Some(CodeLang::Curl));
    assert_eq!(lang.unwrap().ops().len(), 5);
    for op in lang.unwrap().ops() {
        assert!(matches!(op, CodeOp::Curl(_)));
        assert!(op.apply(POST).is_some());
    }
}
