//! Link card parser and site detection, on made-up URLs only.

use super::*;

fn card(text: &str) -> LinkCard {
    parse(text).unwrap_or_else(|| panic!("not a link: {text}"))
}

fn site(text: &str) -> Option<String> {
    card(text).site.clone()
}

#[test]
fn only_one_url_is_a_link() {
    for text in [
        "https://example.com",
        "  https://example.com/a/b?x=1#top \n",
        "HTTP://Example.COM/x",
        "file:///Users/someone/notes.txt",
        "file://localhost/tmp/a.txt",
        "http://localhost:3000/api",
        "localhost:8080",
        "localhost:5173/app",
        "example.com/path",
        "https://example.com/report.pdf",
        "https://192.168.1.20:8443/admin",
        "http://[::1]:9000/",
        "https://bücher.example/",
    ] {
        assert!(is_link(text), "{text}");
    }
    for text in [
        "",
        "see https://example.com here",
        "https://example.com https://example.org",
        "ftp://example.com/file",
        "javascript:alert(1)",
        "mailto:someone@example.com",
        "https://",
        "https:///path",
        "https://:80/",
        "https://example.com:http/",
        "file://",
        "file:///",
        "file://server/share",
        "just words",
        "notes.txt",
        "https://exa<mple.com/",
    ] {
        assert!(!is_link(text), "{text}");
    }
}

#[test]
fn domain_drops_www_and_keeps_the_port() {
    let link = card("https://www.Example.com:8443/a");
    assert_eq!(link.host, "example.com");
    assert_eq!(link.domain(), "example.com:8443");
    assert_eq!(link.copy_domain().as_deref(), Some("example.com"));
    assert_eq!(link.place, Place::Web);
    assert_eq!(link.scheme.as_deref(), Some("https"));
}

#[test]
fn local_and_file_places() {
    assert_eq!(card("http://localhost:3000").place, Place::Local);
    assert_eq!(card("https://10.0.0.1/x").place, Place::Local);
    assert_eq!(card("http://[::1]:9000/").place, Place::Local);
    assert_eq!(card("http://[::1]:9000/").domain(), "[::1]:9000");
    assert_eq!(card("https://256.1.1.1/").place, Place::Web);
    let file = card("file:///Users/someone/notes.txt");
    assert_eq!(file.place, Place::File);
    assert_eq!(file.path, "/Users/someone/notes.txt");
    assert_eq!(file.copy_domain(), None);
    assert_eq!(Place::Web.symbol(), "globe");
    assert_eq!(Place::Local.symbol(), "network");
}

#[test]
fn params_are_names_only_once_and_capped() {
    let link = card("https://example.com/s?q=fiets&page=2&q=auto&sort=new&lang=nl&size=10");
    assert_eq!(link.params, ["q", "page", "sort", "lang", "size"]);
    let lines = link.lines();
    assert!(lines.contains(&Line::Params(vec![
        "q".into(),
        "page".into(),
        "sort".into(),
        "lang".into(),
        "+1".into()
    ])));
    let text = link.text();
    for value in ["fiets", "auto", "new", "10"] {
        assert!(!text.contains(value), "{value} in {text}");
    }
}

#[test]
fn path_is_decoded_and_cut_in_the_middle() {
    let link = card("https://example.com/a%20b/c");
    assert_eq!(link.path, "/a b/c");
    let long = format!("https://example.com/{}/end", "x".repeat(200));
    let lines = card(&long).lines();
    let Some(Line::Path(path)) = lines.iter().find(|line| matches!(line, Line::Path(_))) else {
        panic!("no path");
    };
    assert_eq!(path.chars().count(), MAX_PATH);
    assert!(path.starts_with("/xxx") && path.ends_with("x/end") && path.contains('…'));
    assert_eq!(middle_cut("abcdef", 6), "abcdef");
    assert_eq!(middle_cut("abcdefghij", 5), "ab…ij");
    // No path line for `/` or nothing.
    assert!(
        !card("https://example.com/")
            .lines()
            .iter()
            .any(|line| matches!(line, Line::Path(_)))
    );
}

#[test]
fn scheme_other_than_https_is_a_grey_badge() {
    let badges = card("http://example.com").badges();
    assert_eq!(
        badges,
        [Badge {
            text: "http".into(),
            warn: false
        }]
    );
    assert!(card("https://example.com").badges().is_empty());
    assert!(card("example.com/x").badges().is_empty());
    assert!(
        card("file:///tmp/a.txt")
            .badges()
            .iter()
            .any(|badge| badge.text == "file" && !badge.warn)
    );
}

#[test]
fn token_parameters_are_flagged() {
    for url in [
        "https://example.com/cb?token=abc",
        "https://example.com/cb?key=abc",
        "https://example.com/f?sig=abc",
        "https://example.com/cb?code=abc",
        "https://example.com/cb#access_token=abc&state=1",
        "https://example.com/x?github_token=abc",
        "https://example.com/x?session=0123456789abcdef0123456789abcdef",
        "https://example.com/x?t=Ab3dEf6hIj9kLm2nOp5qRs8t",
        "https://example.com/x?jwt=eyJhbGciOiJub25lIn0.eyJzdWIiOiIxIn0.",
        "https://example.com/verify/eyJhbGciOiJub25lIn0.eyJzdWIiOiIxIn0.",
    ] {
        let link = card(url);
        assert!(link.token, "{url}");
        assert!(link.asks_before_open(), "{url}");
        assert!(
            link.badges().first().is_some_and(|badge| badge.warn),
            "{url}"
        );
    }
    for url in [
        "https://example.com/s?q=fiets&page=2",
        "https://example.com/watch?id=12345",
        "https://example.com/#section-2",
    ] {
        assert!(!card(url).token, "{url}");
        assert!(!card(url).asks_before_open(), "{url}");
    }
}

#[test]
fn tracking_follows_clean_url() {
    let link = card("https://example.com/a?utm_source=news&id=7");
    assert!(link.tracking);
    assert!(
        !link
            .badges()
            .iter()
            .find(|b| !b.warn)
            .unwrap()
            .text
            .is_empty()
    );
    assert!(!card("https://example.com/a?id=7").tracking);
    assert_eq!(
        cleaned("https://example.com/a?utm_source=news&id=7"),
        "https://example.com/a?id=7"
    );
}

#[test]
fn credentials_are_flagged_and_never_shown() {
    let link = card("https://deploy:hunter2@example.com/repo");
    assert!(link.credentials);
    assert_eq!(link.domain(), "example.com");
    let text = link.text();
    assert!(
        !text.contains("hunter2") && !text.contains("deploy"),
        "{text}"
    );
    assert!(!link.ax_label().contains("hunter2"));
    assert!(link.badges().iter().any(|badge| badge.warn));
    assert_eq!(
        cleaned("https://deploy:hunter2@example.com/repo?utm_medium=x"),
        "https://example.com/repo"
    );
    // An `@` in the path is not a user.
    assert!(!card("https://example.com/@someone").credentials);
}

#[test]
fn punycode_domains_show_the_xn_form() {
    assert_eq!(punycode("bücher").as_deref(), Some("bcher-kva"));
    assert_eq!(punycode("münchen").as_deref(), Some("mnchen-3ya"));
    assert_eq!(punycode("例え").as_deref(), Some("r8jz45g"));
    let link = card("https://bücher.example/");
    assert_eq!(link.host, "xn--bcher-kva.example");
    assert!(link.punycode);
    assert!(link.text().starts_with("xn--bcher-kva.example"));
    let already = card("https://xn--80ak6aa92e.com/");
    assert!(already.punycode);
    assert!(!card("https://example.com").punycode);
    // Lookalike Cyrillic `а` in a familiar name.
    let lookalike = card("https://pаypal.com/login");
    assert!(lookalike.punycode && lookalike.host.starts_with("xn--"));
}

#[test]
fn odd_host_characters_are_flagged() {
    assert!(card("https://exa%6Dple.com/").odd_host);
    assert!(card("https://my_host.example.com/").odd_host);
    assert!(!card("https://my-host.example.com/").odd_host);
}

#[test]
fn github_titles() {
    assert_eq!(site("https://github.com").as_deref(), Some("GitHub"));
    assert_eq!(
        site("https://github.com/acme").as_deref(),
        Some("GitHub · acme")
    );
    assert_eq!(
        site("https://github.com/acme/app").as_deref(),
        Some("GitHub · acme/app")
    );
    assert_eq!(
        site("https://github.com/acme/app/pull/42").as_deref(),
        Some("GitHub · acme/app · PR #42")
    );
    assert_eq!(
        site("https://github.com/acme/app/pull/42/files").as_deref(),
        Some("GitHub · acme/app · PR #42")
    );
    assert_eq!(
        site("https://github.com/acme/app/issues/7").as_deref(),
        Some("GitHub · acme/app · Issue #7")
    );
    assert_eq!(
        site("https://github.com/acme/app/commit/0123456789abcdef").as_deref(),
        Some("GitHub · acme/app · Commit 0123456")
    );
    assert_eq!(
        site("https://github.com/acme/app/blob/main/src/main.rs").as_deref(),
        Some("GitHub · acme/app · main.rs")
    );
    assert_eq!(
        site("https://github.com/acme/app.git").as_deref(),
        Some("GitHub · acme/app")
    );
}

#[test]
fn gitlab_titles() {
    assert_eq!(
        site("https://gitlab.com/group/sub/app/-/merge_requests/12").as_deref(),
        Some("GitLab · group/sub/app · MR !12")
    );
    assert_eq!(
        site("https://gitlab.example.org/team/app/-/issues/3").as_deref(),
        Some("GitLab · team/app · Issue #3")
    );
    assert_eq!(
        site("https://gitlab.com/team/app/-/blob/main/README.md").as_deref(),
        Some("GitLab · team/app · README.md")
    );
    assert_eq!(
        site("https://gitlab.com/team/app").as_deref(),
        Some("GitLab · team/app")
    );
}

#[test]
fn jira_titles() {
    assert_eq!(
        site("https://acme.atlassian.net/browse/ABC-123").as_deref(),
        Some("Jira · ABC-123")
    );
    assert_eq!(
        site("https://acme.atlassian.net/jira/software/projects/ABC/boards/1?selectedIssue=ABC-9")
            .as_deref(),
        Some("Jira · ABC-9")
    );
    assert_eq!(
        site("https://jira.example.org/browse/OPS-7").as_deref(),
        Some("Jira · OPS-7")
    );
    assert_eq!(site("https://example.org/browse/abc-1"), None);
}

#[test]
fn other_known_sites() {
    assert_eq!(
        site("https://www.notion.so/acme/Launch-Plan-0123456789abcdef0123456789abcdef").as_deref(),
        Some("Notion · Launch Plan")
    );
    assert_eq!(
        site("https://docs.google.com/document/d/FAKEID/edit").as_deref(),
        Some("Google Docs")
    );
    assert_eq!(
        site("https://docs.google.com/spreadsheets/d/FAKEID/edit#gid=0").as_deref(),
        Some("Google Sheets")
    );
    assert_eq!(
        site("https://www.figma.com/design/FAKEKEY/Onboarding-Flow?node-id=1-2").as_deref(),
        Some("Figma · Onboarding Flow")
    );
    assert_eq!(
        site("https://stackoverflow.com/questions/12345/how-to-parse-a-url").as_deref(),
        Some("Stack Overflow · how to parse a url")
    );
    assert_eq!(
        site("https://www.youtube.com/watch?v=bEN9Dyg48b0").as_deref(),
        Some("YouTube · Video")
    );
    assert_eq!(
        site("https://youtu.be/bEN9Dyg48b0").as_deref(),
        Some("YouTube · Video")
    );
    assert!(site("http://localhost:3000/api").unwrap().contains("3000"));
    assert_eq!(site("http://localhost/").as_deref(), Some("localhost"));
    assert_eq!(site("https://example.com/questions/1"), None);
}

#[test]
fn lines_and_text_follow_the_layout() {
    let link = card("http://github.com/acme/app/pull/42?utm_source=x&tab=files");
    let lines = link.lines();
    assert_eq!(lines[0], Line::Domain("github.com".into()));
    assert_eq!(lines[1], Line::Site("GitHub · acme/app · PR #42".into()));
    assert_eq!(lines[2], Line::Path("/acme/app/pull/42".into()));
    assert_eq!(
        lines[3],
        Line::Params(vec!["utm_source".into(), "tab".into()])
    );
    assert!(matches!(&lines[4], Line::Badges(badges) if badges.len() == 2));
    assert_eq!(link.text().lines().count(), 5);
    let ax = link.ax_label();
    assert!(ax.contains("github.com") && ax.contains("PR #42"), "{ax}");
}

#[test]
fn link_in_text_finds_exactly_one() {
    assert_eq!(
        link_in_text("Kijk op https://example.com/a. Groet"),
        Some("https://example.com/a")
    );
    assert_eq!(
        link_in_text("https://a.example https://b.example and"),
        None
    );
    assert_eq!(link_in_text("no link here"), None);
    assert_eq!(link_in_text("https://example.com/a"), None);
}
