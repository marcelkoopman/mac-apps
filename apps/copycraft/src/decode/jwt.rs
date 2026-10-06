//! JWT: `header.payload.signature`, where the header base64url-decodes to a JSON object with
//! `"alg"`. Shows the header and the payload as pretty JSON and reads `exp`, `nbf` and `iat` as
//! dates with a status. The signature is never checked, and the view says so.

use serde_json::{Map, Value as JsonValue};

use super::clock::{self, Env};

/// The decoded view of a JWT, or `None` when `text` is not one.
pub fn decode(text: &str, env: &Env) -> Option<String> {
    let (header, payload) = parts(text)?;
    let mut out = String::new();
    out.push_str(env.pick("Handtekening niet gecontroleerd", "Signature not verified"));
    out.push('\n');
    if let Some(alg) = header.get("alg").and_then(JsonValue::as_str) {
        out.push_str(&format!("alg: {alg}\n"));
    }
    let times = time_lines(&payload, env);
    if !times.is_empty() {
        out.push('\n');
        out.push_str(&times.join("\n"));
        out.push('\n');
    }
    out.push('\n');
    out.push_str("Header\n");
    out.push_str(&serde_json::to_string_pretty(&JsonValue::Object(header)).ok()?);
    out.push_str("\n\nPayload\n");
    out.push_str(&serde_json::to_string_pretty(&JsonValue::Object(payload)).ok()?);
    Some(out)
}

/// Header and payload objects. Three dot-separated parts of the base64url alphabet (the
/// signature may be empty, for `alg: none`), the header JSON with `alg`, the payload JSON.
fn parts(text: &str) -> Option<(Map<String, JsonValue>, Map<String, JsonValue>)> {
    let mut split = text.split('.');
    let (header, payload, signature) = (split.next()?, split.next()?, split.next()?);
    if split.next().is_some() || header.is_empty() || payload.is_empty() {
        return None;
    }
    if ![header, payload, signature]
        .iter()
        .all(|part| is_b64url(part))
    {
        return None;
    }
    let header = json_object(header)?;
    if !header.contains_key("alg") {
        return None;
    }
    Some((header, json_object(payload)?))
}

fn is_b64url(part: &str) -> bool {
    part.trim_end_matches('=')
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn json_object(part: &str) -> Option<Map<String, JsonValue>> {
    match serde_json::from_slice(&super::base64::bytes(part)?).ok()? {
        JsonValue::Object(map) => Some(map),
        _ => None,
    }
}

/// `iat`, `nbf`, `exp` (in that order) as dates with a status; a note when there is no `exp`.
fn time_lines(payload: &Map<String, JsonValue>, env: &Env) -> Vec<String> {
    let now = env.now_ms.div_euclid(1000);
    let mut lines = Vec::new();
    for claim in ["iat", "nbf", "exp"] {
        let Some(secs) = payload.get(claim).and_then(claim_secs) else {
            continue;
        };
        let ahead = secs.saturating_sub(now);
        let amount = clock::span(ahead.unsigned_abs(), env.lang);
        let (name, status) = match claim {
            "iat" => (
                env.pick("Uitgegeven (iat)", "Issued (iat)"),
                clock::relative(secs, env),
            ),
            "nbf" if ahead > 0 => (
                env.pick("Niet vóór (nbf)", "Not before (nbf)"),
                env.pick(
                    &format!("nog niet geldig (over {amount})"),
                    &format!("not valid yet (in {amount})"),
                )
                .to_string(),
            ),
            "nbf" => (
                env.pick("Niet vóór (nbf)", "Not before (nbf)"),
                env.pick(
                    &format!("al geldig ({amount} geleden)"),
                    &format!("in effect ({amount} ago)"),
                )
                .to_string(),
            ),
            _ if ahead > 0 => (
                env.pick("Verloopt (exp)", "Expires (exp)"),
                env.pick(
                    &format!("nog {amount} geldig"),
                    &format!("valid for {amount}"),
                )
                .to_string(),
            ),
            _ => (
                env.pick("Verloopt (exp)", "Expires (exp)"),
                env.pick(
                    &format!("verlopen ({amount} geleden)"),
                    &format!("expired ({amount} ago)"),
                )
                .to_string(),
            ),
        };
        let [local, utc] = clock::moment_lines(secs, None, env);
        lines.push(format!("{name}: {status}\n  {local}\n  {utc}"));
    }
    if !payload.contains_key("exp") {
        lines.push(
            env.pick("Geen vervaldatum (exp)", "No expiry (exp)")
                .to_string(),
        );
    }
    lines
}

/// A NumericDate: whole seconds (a fraction is dropped). Out of a sane range: not a date.
fn claim_secs(value: &JsonValue) -> Option<i64> {
    let secs = value
        .as_i64()
        .or_else(|| value.as_f64().map(|f| f as i64))?;
    // Up to the year 9999.
    (0..=253_402_300_799).contains(&secs).then_some(secs)
}

#[cfg(test)]
mod tests {
    use super::decode;
    use crate::decode::clock::tests::env;
    use crate::decode::testdata;
    use crate::locale::Lang;

    #[test]
    fn expired_token_shows_header_payload_and_dates() {
        let token = testdata("jwt_expired.txt");
        let out = decode(&token, &env(Lang::En)).expect("jwt");
        assert!(out.starts_with("Signature not verified\n"), "{out}");
        assert!(out.contains("\"alg\": \"HS256\""));
        assert!(out.contains("\"name\": \"Copycraft Test\""));
        assert!(out.contains("Header\n{"));
        assert!(out.contains("Payload\n{"));
        // exp is 2026-10-01 09:00 UTC; the test clock is 2026-10-06 08:00 UTC, local UTC+2.
        assert!(out.contains("Expires (exp): expired (4 days ago)"), "{out}");
        assert!(out.contains("01/10/2026 11:00:00  local time (UTC+02:00)"));
        assert!(out.contains("01/10/2026 09:00:00  UTC"));
        assert!(out.contains("Issued (iat): 5 days ago"));
        let nl = decode(&token, &env(Lang::Nl)).expect("jwt");
        assert!(nl.starts_with("Handtekening niet gecontroleerd\n"));
        assert!(
            nl.contains("Verloopt (exp): verlopen (4 dagen geleden)"),
            "{nl}"
        );
        assert!(nl.contains("Uitgegeven (iat): 5 dagen geleden"));
    }

    #[test]
    fn valid_and_not_yet_valid_tokens() {
        let mut clock = env(Lang::Nl);
        let valid = testdata("jwt_valid_until_2099.txt");
        let out = decode(&valid, &clock).expect("jwt");
        assert!(out.contains("Verloopt (exp): nog 72 jaar geldig"), "{out}");
        // 30 minutes before exp: minutes.
        clock.now_ms = (4_092_595_200 - 30 * 60) * 1000;
        let out = decode(&valid, &clock).expect("jwt");
        assert!(out.contains("nog 30 minuten geldig"), "{out}");
        clock.now_ms = (4_092_595_200 - 5 * 3600) * 1000;
        clock.lang = Lang::En;
        let out = decode(&valid, &clock).expect("jwt");
        assert!(out.contains("Expires (exp): valid for 5 hours"), "{out}");
        let later = decode(&testdata("jwt_not_before_future.txt"), &env(Lang::En)).expect("jwt");
        assert!(
            later.contains("Not before (nbf): not valid yet (in 71 years)"),
            "{later}"
        );
        let nl = decode(&testdata("jwt_not_before_future.txt"), &env(Lang::Nl)).expect("jwt");
        assert!(nl.contains("Niet vóór (nbf): nog niet geldig (over 71 jaar)"));
    }

    #[test]
    fn missing_exp_is_noted() {
        let token = "eyJhbGciOiJub25lIn0.eyJzdWIiOiJ0ZXN0In0.";
        let out = decode(token, &env(Lang::En)).expect("unsigned jwt");
        assert!(out.contains("alg: none"));
        assert!(out.contains("No expiry (exp)"));
    }

    #[test]
    fn rejects_non_jwts() {
        for name in ["jwt_invalid_no_alg.txt", "jwt_invalid_not_json.txt"] {
            assert!(decode(&testdata(name), &env(Lang::En)).is_none(), "{name}");
        }
        assert!(decode("a.b.c", &env(Lang::En)).is_none());
        assert!(decode("example.com.au", &env(Lang::En)).is_none());
        assert!(decode("1.2.3", &env(Lang::En)).is_none());
    }
}
