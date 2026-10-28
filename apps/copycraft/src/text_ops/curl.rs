//! Text ▾ › Code on a copied `curl` command: the same request as JavaScript `fetch`, Python
//! `requests`, HTTPie, Rust `reqwest` or Swift `URLSession`. Text only: nothing is run and
//! nothing goes on the network.
//!
//! Read like a shell would split it (single and double quotes, `\` escapes, `\`-continued
//! lines), with the options that shape a request: `-X`, `-H`, `-d`/`--data`/`--data-raw`/
//! `--data-binary`/`--data-urlencode`, `--json`, `-u`, `-F`, `--url`, `-G`, plus `-b` (a
//! Cookie header), `-A` (User-Agent) and `-e` (Referer). Options that only change how curl
//! runs (`-s`, `-L`, `-k`, `-o file`, …) are skipped. A request with an Authorization or
//! Cookie header or `-u` keeps the credential label on its result ([`carries_credential`]).
//! Neither the command nor the result is ever logged.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

/// The target language of a conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurlTarget {
    Fetch,
    Python,
    Httpie,
    Reqwest,
    Swift,
}

/// One `-F` field: text, or a file (`name=@path`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormValue {
    Text(String),
    File(String),
}

/// The request a curl command makes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// The `-d`/`--json` body (several `-d` joined with `&`, as curl does).
    pub body: Option<String>,
    /// `--json`: the body is JSON.
    pub json: bool,
    /// `-u user:password` (the password may be left out).
    pub user: Option<(String, Option<String>)>,
    pub form: Vec<(String, FormValue)>,
}

/// A copy that is a curl command: its first word is `curl` (a `$ ` prompt before it is fine).
pub fn looks_like_curl(text: &str) -> bool {
    let text = text.trim_start();
    let text = text.strip_prefix("$ ").unwrap_or(text);
    text.starts_with("curl ") || text.starts_with("curl\t") || text.starts_with("curl \\\n")
}

/// Shell words of `text`: quotes and escapes resolved, `\`-newline joined.
fn words(text: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('\n') => {}
                Some('\r') if chars.peek() == Some(&'\n') => {
                    chars.next();
                }
                Some(next) => {
                    word.push(next);
                    in_word = true;
                }
                None => {}
            },
            '\'' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        other => word.push(other),
                    }
                }
            }
            '$' if chars.peek() == Some(&'\'') => {
                // `$'…'`: C escapes.
                chars.next();
                in_word = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        '\\' => match chars.next()? {
                            'n' => word.push('\n'),
                            't' => word.push('\t'),
                            'r' => word.push('\r'),
                            other => word.push(other),
                        },
                        other => word.push(other),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => match chars.next()? {
                            '\n' => {}
                            other @ ('"' | '\\' | '$' | '`') => word.push(other),
                            other => {
                                word.push('\\');
                                word.push(other);
                            }
                        },
                        other => word.push(other),
                    }
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    out.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            c => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        out.push(word);
    }
    Some(out)
}

/// Options that take a value but do not change the request.
const SKIPPED_WITH_VALUE: &[&str] = &[
    "-o",
    "--output",
    "-m",
    "--max-time",
    "--connect-timeout",
    "-w",
    "--write-out",
    "--retry",
    "-x",
    "--proxy",
    "--cacert",
    "-E",
    "--cert",
    "--key",
    "-c",
    "--cookie-jar",
    "--resolve",
    "-T",
    "--upload-file",
    "--limit-rate",
    "-r",
    "--range",
];

/// The request `text` (a curl command) makes; `None` when it is not one or has no URL.
pub fn parse(text: &str) -> Option<Request> {
    if !looks_like_curl(text) {
        return None;
    }
    let text = text.trim_start();
    let text = text.strip_prefix("$ ").unwrap_or(text);
    let words = words(text)?;
    let mut args = words.into_iter().skip(1);
    let mut request = Request::default();
    let mut data: Vec<String> = Vec::new();
    let mut get = false;
    let mut method: Option<String> = None;
    while let Some(arg) = args.next() {
        // `--name=value` and `-Xvalue`.
        let (flag, inline) = if let Some(rest) = arg.strip_prefix("--") {
            match rest.split_once('=') {
                Some((name, value)) => (format!("--{name}"), Some(value.to_string())),
                None => (arg.clone(), None),
            }
        } else if arg.starts_with('-')
            && !arg.starts_with("--")
            && arg.len() > 2
            && arg.is_char_boundary(2)
            && "XHdufFAeb".contains(&arg[1..2])
        {
            (arg[..2].to_string(), Some(arg[2..].to_string()))
        } else {
            (arg.clone(), None)
        };
        let mut value = |inline: Option<String>| inline.or_else(|| args.next());
        match flag.as_str() {
            "-X" | "--request" => method = Some(value(inline)?.to_ascii_uppercase()),
            "-H" | "--header" => {
                let header = value(inline)?;
                let (name, rest) = header.split_once(':')?;
                request
                    .headers
                    .push((name.trim().to_string(), rest.trim().to_string()));
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii"
            | "--data-urlencode" => data.push(value(inline)?),
            "--json" => {
                data.push(value(inline)?);
                request.json = true;
            }
            "-u" | "--user" => {
                let user = value(inline)?;
                request.user = Some(match user.split_once(':') {
                    Some((name, password)) => (name.to_string(), Some(password.to_string())),
                    None => (user, None),
                });
            }
            "-F" | "--form" => {
                let field = value(inline)?;
                let (name, rest) = field.split_once('=')?;
                let form_value = match rest.strip_prefix('@') {
                    Some(path) => FormValue::File(path.split(';').next()?.to_string()),
                    None => FormValue::Text(rest.to_string()),
                };
                request.form.push((name.to_string(), form_value));
            }
            "--url" => request.url = value(inline)?,
            "-G" | "--get" => get = true,
            "-A" | "--user-agent" => request.headers.push(("User-Agent".into(), value(inline)?)),
            "-e" | "--referer" => request.headers.push(("Referer".into(), value(inline)?)),
            "-b" | "--cookie" => request.headers.push(("Cookie".into(), value(inline)?)),
            flag if SKIPPED_WITH_VALUE.contains(&flag) => {
                value(inline)?;
            }
            flag if flag.starts_with('-') && flag.len() > 1 => {}
            _ if request.url.is_empty() => request.url = arg,
            _ => {}
        }
    }
    if request.url.is_empty() {
        return None;
    }
    if request.json {
        for (name, value) in [
            ("Content-Type", "application/json"),
            ("Accept", "application/json"),
        ] {
            if !request
                .headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case(name))
            {
                request.headers.push((name.into(), value.into()));
            }
        }
    }
    let joined = (!data.is_empty()).then(|| data.join("&"));
    if get {
        if let Some(query) = joined {
            let separator = if request.url.contains('?') { '&' } else { '?' };
            request.url = format!("{}{separator}{query}", request.url);
        }
    } else {
        request.body = joined;
    }
    let has_body = request.body.is_some() || !request.form.is_empty();
    request.method = method.unwrap_or_else(|| if has_body { "POST" } else { "GET" }.into());
    if request.body.is_some()
        && !request.json
        && request.form.is_empty()
        && !request
            .headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case("content-type"))
    {
        // curl sends `-d` as a form unless told otherwise.
        request.headers.push((
            "Content-Type".into(),
            "application/x-www-form-urlencoded".into(),
        ));
    }
    Some(request)
}

/// The command sends a credential: an Authorization or Cookie header, `-b` or `-u`. Its
/// conversion gets the credential label whatever the scanner finds in it.
pub fn carries_credential(text: &str) -> bool {
    parse(text).is_some_and(|request| {
        request.user.is_some()
            || request.headers.iter().any(|(name, _)| {
                [
                    "authorization",
                    "cookie",
                    "proxy-authorization",
                    "x-api-key",
                ]
                .contains(&name.to_ascii_lowercase().as_str())
            })
    })
}

/// The request in `target`'s language; `None` when `text` is not a curl command.
pub fn convert(text: &str, target: CurlTarget) -> Option<String> {
    let request = parse(text)?;
    Some(match target {
        CurlTarget::Fetch => to_fetch(&request),
        CurlTarget::Python => to_python(&request),
        CurlTarget::Httpie => to_httpie(&request),
        CurlTarget::Reqwest => to_reqwest(&request),
        CurlTarget::Swift => to_swift(&request),
    })
}

/// A JSON (and JavaScript) string literal.
fn js(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_default()
}

/// The body as JSON when it parses as an object or array.
fn json_body(request: &Request) -> Option<serde_json::Value> {
    let body = request.body.as_deref()?;
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    (value.is_object() || value.is_array()).then_some(value)
}

fn basic(user: &(String, Option<String>)) -> String {
    format!("{}:{}", user.0, user.1.as_deref().unwrap_or(""))
}

fn to_fetch(request: &Request) -> String {
    let mut out = String::new();
    if !request.form.is_empty() {
        out.push_str("const form = new FormData();\n");
        for (name, value) in &request.form {
            match value {
                FormValue::Text(text) => {
                    out.push_str(&format!("form.append({}, {});\n", js(name), js(text)))
                }
                FormValue::File(path) => out.push_str(&format!(
                    "form.append({}, file); // the file {}\n",
                    js(name),
                    js(path)
                )),
            }
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "const response = await fetch({}, {{\n",
        js(&request.url)
    ));
    out.push_str(&format!("  method: {},\n", js(&request.method)));
    let headers: Vec<&(String, String)> = request
        .headers
        .iter()
        // FormData sets its own Content-Type with the boundary.
        .filter(|(name, _)| request.form.is_empty() || !name.eq_ignore_ascii_case("content-type"))
        .collect();
    if !headers.is_empty() || request.user.is_some() {
        out.push_str("  headers: {\n");
        for (name, value) in headers {
            out.push_str(&format!("    {}: {},\n", js(name), js(value)));
        }
        if let Some(user) = &request.user {
            out.push_str(&format!(
                "    \"Authorization\": \"Basic \" + btoa({}),\n",
                js(&basic(user))
            ));
        }
        out.push_str("  },\n");
    }
    if !request.form.is_empty() {
        out.push_str("  body: form,\n");
    } else if let Some(value) = json_body(request) {
        let pretty = serde_json::to_string_pretty(&value).unwrap_or_default();
        out.push_str(&format!(
            "  body: JSON.stringify({}),\n",
            pretty.replace('\n', "\n  ")
        ));
    } else if let Some(body) = &request.body {
        out.push_str(&format!("  body: {},\n", js(body)));
    }
    out.push_str("});\n");
    out
}

/// A JSON value as a Python literal (`True`, `False`, `None`).
fn python_value(value: &serde_json::Value, indent: usize) -> String {
    use serde_json::Value;
    let pad = "    ".repeat(indent + 1);
    let end = "    ".repeat(indent);
    match value {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => js(text),
        Value::Array(items) if items.is_empty() => "[]".into(),
        Value::Array(items) => {
            let inner: Vec<String> = items
                .iter()
                .map(|item| format!("{pad}{}", python_value(item, indent + 1)))
                .collect();
            format!("[\n{},\n{end}]", inner.join(",\n"))
        }
        Value::Object(map) if map.is_empty() => "{}".into(),
        Value::Object(map) => {
            let inner: Vec<String> = map
                .iter()
                .map(|(key, item)| format!("{pad}{}: {}", js(key), python_value(item, indent + 1)))
                .collect();
            format!("{{\n{},\n{end}}}", inner.join(",\n"))
        }
    }
}

fn to_python(request: &Request) -> String {
    let mut out = String::from("import requests\n\n");
    let mut args = vec![js(&request.url)];
    let json = json_body(request);
    let headers: Vec<&(String, String)> = request
        .headers
        .iter()
        .filter(|(name, _)| {
            // `json=` and `files=` set their own Content-Type.
            !((json.is_some() || !request.form.is_empty())
                && name.eq_ignore_ascii_case("content-type"))
        })
        .collect();
    if !headers.is_empty() {
        out.push_str("headers = {\n");
        for (name, value) in headers {
            out.push_str(&format!("    {}: {},\n", js(name), js(value)));
        }
        out.push_str("}\n");
        args.push("headers=headers".into());
    }
    if !request.form.is_empty() {
        let texts: Vec<_> = request
            .form
            .iter()
            .filter_map(|(name, value)| match value {
                FormValue::Text(text) => Some((name, text)),
                FormValue::File(_) => None,
            })
            .collect();
        let files: Vec<_> = request
            .form
            .iter()
            .filter_map(|(name, value)| match value {
                FormValue::File(path) => Some((name, path)),
                FormValue::Text(_) => None,
            })
            .collect();
        if !texts.is_empty() {
            out.push_str("data = {\n");
            for (name, text) in texts {
                out.push_str(&format!("    {}: {},\n", js(name), js(text)));
            }
            out.push_str("}\n");
            args.push("data=data".into());
        }
        if !files.is_empty() {
            out.push_str("files = {\n");
            for (name, path) in files {
                out.push_str(&format!("    {}: open({}, \"rb\"),\n", js(name), js(path)));
            }
            out.push_str("}\n");
            args.push("files=files".into());
        }
    } else if let Some(value) = &json {
        out.push_str(&format!("json_data = {}\n", python_value(value, 0)));
        args.push("json=json_data".into());
    } else if let Some(body) = &request.body {
        out.push_str(&format!("data = {}\n", js(body)));
        args.push("data=data".into());
    }
    if let Some((name, password)) = &request.user {
        args.push(format!(
            "auth=({}, {})",
            js(name),
            js(password.as_deref().unwrap_or(""))
        ));
    }
    let call = match request.method.as_str() {
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" => {
            format!("requests.{}(", request.method.to_ascii_lowercase())
        }
        other => format!("requests.request({}, ", js(other)),
    };
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
    out.push_str(&format!("response = {call}{})\n", args.join(", ")));
    out
}

/// A word for a POSIX shell: in single quotes unless it is plain.
fn shell(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:@%+,=".contains(c))
    {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

fn to_httpie(request: &Request) -> String {
    let mut parts = vec!["http".to_string()];
    if !request.form.is_empty() {
        parts.push("--form".into());
    }
    if let Some(user) = &request.user {
        parts.push("-a".into());
        parts.push(shell(&basic(user)));
    }
    if request.form.is_empty()
        && let Some(body) = &request.body
    {
        parts.push(format!("--raw={}", shell(body)));
    }
    parts.push(request.method.clone());
    parts.push(shell(&request.url));
    for (name, value) in &request.headers {
        if !request.form.is_empty() && name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        parts.push(shell(&format!("{name}:{value}")));
    }
    for (name, value) in &request.form {
        parts.push(match value {
            FormValue::Text(text) => shell(&format!("{name}={text}")),
            FormValue::File(path) => shell(&format!("{name}@{path}")),
        });
    }
    let mut out = parts.join(" ");
    out.push('\n');
    out
}

/// A Rust string literal.
fn rust(text: &str) -> String {
    format!("{text:?}")
}

fn to_reqwest(request: &Request) -> String {
    let mut out = String::from("let client = reqwest::Client::new();\n");
    if !request.form.is_empty() {
        out.push_str("let form = reqwest::multipart::Form::new()");
        for (name, value) in &request.form {
            match value {
                FormValue::Text(text) => {
                    out.push_str(&format!("\n    .text({}, {})", rust(name), rust(text)))
                }
                FormValue::File(path) => out.push_str(&format!(
                    "\n    .file({}, {}).await?",
                    rust(name),
                    rust(path)
                )),
            }
        }
        out.push_str(";\n");
    }
    out.push_str("let response = client\n");
    match request.method.as_str() {
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" => out.push_str(&format!(
            "    .{}({})\n",
            request.method.to_ascii_lowercase(),
            rust(&request.url)
        )),
        other => out.push_str(&format!(
            "    .request(reqwest::Method::from_bytes({})?, {})\n",
            format_args!("b{}", rust(other)),
            rust(&request.url)
        )),
    }
    for (name, value) in &request.headers {
        if !request.form.is_empty() && name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        out.push_str(&format!("    .header({}, {})\n", rust(name), rust(value)));
    }
    if let Some((name, password)) = &request.user {
        let password = match password {
            Some(password) => format!("Some({})", rust(password)),
            None => "None::<&str>".into(),
        };
        out.push_str(&format!("    .basic_auth({}, {password})\n", rust(name)));
    }
    if !request.form.is_empty() {
        out.push_str("    .multipart(form)\n");
    } else if let Some(body) = &request.body {
        out.push_str(&format!("    .body({})\n", rust(body)));
    }
    out.push_str("    .send()\n    .await?;\n");
    out
}

/// A Swift string literal (`\(` would interpolate, so every `\` is escaped).
fn swift(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn to_swift(request: &Request) -> String {
    let mut out = format!(
        "var request = URLRequest(url: URL(string: {})!)\nrequest.httpMethod = {}\n",
        swift(&request.url),
        swift(&request.method)
    );
    for (name, value) in &request.headers {
        if !request.form.is_empty() && name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        out.push_str(&format!(
            "request.setValue({}, forHTTPHeaderField: {})\n",
            swift(value),
            swift(name)
        ));
    }
    if let Some(user) = &request.user {
        out.push_str(&format!(
            "request.setValue(\"Basic \" + Data({}.utf8).base64EncodedString(), forHTTPHeaderField: \"Authorization\")\n",
            swift(&basic(user))
        ));
    }
    if !request.form.is_empty() {
        out.push_str("let boundary = UUID().uuidString\n");
        out.push_str("request.setValue(\"multipart/form-data; boundary=\\(boundary)\", forHTTPHeaderField: \"Content-Type\")\n");
        out.push_str("var body = Data()\n");
        for (name, value) in &request.form {
            let name = swift(name);
            let name = &name[1..name.len() - 1];
            match value {
                FormValue::Text(text) => {
                    let text = swift(text);
                    let text = &text[1..text.len() - 1];
                    out.push_str(&format!(
                        "body.append(Data(\"--\\(boundary)\\r\\nContent-Disposition: form-data; name=\\\"{name}\\\"\\r\\n\\r\\n{text}\\r\\n\".utf8))\n"
                    ));
                }
                FormValue::File(path) => {
                    let file = path.rsplit('/').next().unwrap_or(path);
                    let file = swift(file);
                    let file = &file[1..file.len() - 1];
                    out.push_str(&format!(
                        "body.append(Data(\"--\\(boundary)\\r\\nContent-Disposition: form-data; name=\\\"{name}\\\"; filename=\\\"{file}\\\"\\r\\n\\r\\n\".utf8))\n"
                    ));
                    out.push_str(&format!(
                        "body.append(try Data(contentsOf: URL(fileURLWithPath: {})))\n",
                        swift(path)
                    ));
                    out.push_str("body.append(Data(\"\\r\\n\".utf8))\n");
                }
            }
        }
        out.push_str("body.append(Data(\"--\\(boundary)--\\r\\n\".utf8))\n");
        out.push_str("request.httpBody = body\n");
    } else if let Some(body) = &request.body {
        out.push_str(&format!("request.httpBody = Data({}.utf8)\n", swift(body)));
    }
    out.push_str("let (data, response) = try await URLSession.shared.data(for: request)\n");
    out
}

#[cfg(test)]
#[path = "curl_tests.rs"]
mod tests;
