//! Readable names for known sites' link parts, on made-up URLs only.

use super::Meaning::*;
use crate::link_card::{Line, parse};

fn chips(text: &str) -> Vec<String> {
    let link = parse(text).unwrap_or_else(|| panic!("not a link: {text}"));
    link.lines()
        .into_iter()
        .find_map(|line| match line {
            Line::Params(names) => Some(names),
            _ => None,
        })
        .unwrap_or_default()
}

fn has_path(text: &str) -> bool {
    parse(text)
        .unwrap()
        .lines()
        .iter()
        .any(|line| matches!(line, Line::Path(_)))
}

fn meanings(text: &str) -> Vec<super::Meaning> {
    parse(text).unwrap().meanings.clone()
}

#[test]
fn a_youtube_watch_link_shows_its_video_id_not_watch_and_v() {
    let text = "https://www.youtube.com/watch?v=aBcDeFgHiJk&t=42s&list=PLfake&index=3&si=share1";
    assert!(!has_path(text));
    // The ID is on the site line, so no Video-ID chip.
    assert_eq!(meanings(text), [StartTime, Playlist, PlaylistPosition]);
    let link = parse(text).unwrap();
    assert_eq!(link.params, ["si"]);
    assert_eq!(
        link.site_line().as_deref(),
        Some("YouTube · Video · aBcDeFgHiJk")
    );
    let shown = link.text();
    // The video ID is the one value shown; t, list, index and si stay hidden.
    assert!(shown.contains("aBcDeFgHiJk"), "{shown}");
    for hidden in ["/watch", "42s", "PLfake", "share1", "=3"] {
        assert!(!shown.contains(hidden), "{hidden} in {shown}");
    }
    assert!(link.ax_label().contains("aBcDeFgHiJk"));
    assert_eq!(chips(text).len(), 4);
}

#[test]
fn a_value_without_the_video_id_shape_stays_hidden() {
    for text in [
        "https://www.youtube.com/watch?v=tooshort",
        "https://www.youtube.com/watch?v=aBcDeFgHiJk12",
        "https://www.youtube.com/watch?v=aBcDe%2FgHiJ",
    ] {
        let link = parse(text).unwrap();
        assert_eq!(link.video_id, None, "{text}");
        assert_eq!(link.meanings, [VideoId], "{text}");
        let shown = link.text();
        for hidden in ["tooshort", "aBcDeFgHiJk12", "aBcDe"] {
            assert!(!shown.contains(hidden), "{hidden} in {shown}");
        }
    }
}

#[test]
fn youtu_be_shorts_embed_and_playlist() {
    assert!(!has_path("https://youtu.be/aBcDeFgHiJk?t=10"));
    assert_eq!(meanings("https://youtu.be/aBcDeFgHiJk?t=10"), [StartTime]);
    let short = "https://www.youtube.com/shorts/aBcDeFgHiJk";
    assert!(!has_path(short));
    assert_eq!(meanings(short), [Short]);
    assert_eq!(
        parse(short).unwrap().site_line().as_deref(),
        Some("YouTube · Short · aBcDeFgHiJk")
    );
    for shape in ["embed", "live", "v"] {
        let text = format!("https://www.youtube.com/{shape}/aBcDeFgHiJk");
        let link = parse(&text).unwrap();
        assert!(link.meanings.is_empty(), "{text}");
        assert_eq!(link.video_id.as_deref(), Some("aBcDeFgHiJk"), "{text}");
    }
    assert_eq!(
        parse("https://youtu.be/aBcDeFgHiJk")
            .unwrap()
            .video_id
            .as_deref(),
        Some("aBcDeFgHiJk")
    );
    let playlist = "https://www.youtube.com/playlist?list=PLfake";
    assert!(!has_path(playlist));
    assert_eq!(meanings(playlist), [Playlist]);
}

#[test]
fn unknown_parameters_keep_their_raw_chips_after_the_meanings() {
    let text = "https://www.youtube.com/watch?v=aBcDeFgHiJk&t=1&feature=share&pp=x&ab=1&cd=2";
    let chips = chips(text);
    assert_eq!(chips[1..4], ["feature", "pp", "ab"]);
    assert_eq!(chips.last().map(String::as_str), Some("+1"));
    assert_eq!(chips.len(), 5);
    assert_eq!(parse(text).unwrap().params, ["feature", "pp", "ab", "cd"]);
}

#[test]
fn tracking_and_token_labels_stay() {
    let link = parse("https://www.youtube.com/watch?v=aBcDeFgHiJk&utm_source=x").unwrap();
    assert!(link.tracking);
    assert_eq!(link.params, ["utm_source"]);
    let link = parse("https://www.figma.com/design/FAKEKEY/Flow?node-id=1-2&token=abc").unwrap();
    assert!(link.token);
    assert_eq!(link.params, ["token"]);
    assert_eq!(link.meanings, [FileId, SelectedLayer]);
}

#[test]
fn github_gitlab_and_jira_paths_are_on_the_site_line() {
    for text in [
        "https://github.com/acme/app",
        "https://github.com/acme/app/pull/42",
        "https://github.com/acme/app/issues/7",
        "https://github.com/acme/app/commit/0123456789abcdef",
        "https://gitlab.com/group/project/-/merge_requests/7",
        "https://example.atlassian.net/browse/ABC-123",
    ] {
        assert!(!has_path(text), "{text}");
        assert!(meanings(text).is_empty(), "{text}");
    }
    // A file in a repository: the path says which.
    assert!(has_path("https://github.com/acme/app/blob/main/src/lib.rs"));
    assert_eq!(
        meanings("https://github.com/acme/app/pull/42?tab=files"),
        [Tab]
    );
    assert_eq!(
        meanings(
            "https://example.atlassian.net/jira/software/projects/ABC/boards/1?selectedIssue=ABC-9"
        ),
        [SelectedIssue]
    );
}

#[test]
fn google_figma_stack_overflow_and_notion() {
    let doc = "https://docs.google.com/document/d/FAKEDOCID/edit";
    assert!(!has_path(doc));
    assert_eq!(meanings(doc), [DocumentId]);
    assert!(!parse(doc).unwrap().text().contains("FAKEDOCID"));
    assert_eq!(
        meanings("https://docs.google.com/spreadsheets/d/FAKEID/edit?gid=0"),
        [DocumentId, SheetTab]
    );
    assert_eq!(
        meanings("https://drive.google.com/file/d/FAKEID/view"),
        [FileId]
    );
    assert_eq!(
        meanings("https://drive.google.com/drive/folders/FAKEID"),
        [FolderId]
    );
    let figma = "https://www.figma.com/design/FAKEKEY/Onboarding-Flow?node-id=1-2";
    assert!(!has_path(figma));
    assert_eq!(meanings(figma), [FileId, SelectedLayer]);
    let question = "https://stackoverflow.com/questions/12345/how-to-parse-a-url";
    assert!(!has_path(question));
    assert_eq!(meanings(question), [QuestionNumber]);
    let page = "https://www.notion.so/acme/Plan-0123456789abcdef0123456789abcdef?v=fake";
    assert!(!has_path(page));
    assert_eq!(meanings(page), [PageId, DatabaseView]);
}

#[test]
fn other_sites_keep_path_and_raw_names() {
    let text = "https://example.com/watch?v=1&t=2";
    assert!(has_path(text));
    assert!(meanings(text).is_empty());
    assert_eq!(chips(text), ["v", "t"]);
    // A YouTube page that is not a known shape.
    assert!(has_path("https://www.youtube.com/@channel/videos"));
}

#[test]
fn labels_have_both_languages() {
    for meaning in [
        VideoId,
        Short,
        StartTime,
        Playlist,
        PlaylistPosition,
        Tab,
        SearchTerms,
        DocumentId,
        SheetTab,
        FileId,
        FolderId,
        SelectedLayer,
        QuestionNumber,
        SelectedIssue,
        PageId,
        DatabaseView,
    ] {
        assert!(!meaning.label().is_empty(), "{meaning:?}");
        assert!(!meaning.label().starts_with("link_mean"), "{meaning:?}");
    }
}

#[test]
fn facebook_and_instagram_say_what_the_link_is_and_only_a_video_shows_its_id() {
    use super::SocialKind;
    const NUM: &str = "1092344577040057";
    let site = |text: &str| parse(text).unwrap().site.clone();
    // (link, kind, the ID shown: only a Facebook reel or video).
    for (text, kind, id) in [
        (
            "https://www.facebook.com/reel/1092344577040057",
            SocialKind::Reel,
            Some(NUM),
        ),
        (
            "https://www.facebook.com/watch?v=1092344577040057",
            SocialKind::Video,
            Some(NUM),
        ),
        (
            "https://www.facebook.com/someone/posts/1092344577040057",
            SocialKind::Post,
            None,
        ),
        (
            "https://www.facebook.com/someone/videos/1092344577040057",
            SocialKind::Video,
            Some(NUM),
        ),
        (
            "https://www.facebook.com/groups/1092344577040057",
            SocialKind::Group,
            None,
        ),
        (
            "https://www.facebook.com/events/1092344577040057",
            SocialKind::Event,
            None,
        ),
        (
            "https://www.facebook.com/photo/?fbid=1092344577040057&set=a.1",
            SocialKind::Photo,
            None,
        ),
        (
            "https://m.facebook.com/story.php?story_fbid=1092344577040057&id=4",
            SocialKind::Post,
            None,
        ),
        (
            "https://fb.watch/aBcDeF1234/",
            SocialKind::Video,
            Some("aBcDeF1234"),
        ),
        (
            "https://www.instagram.com/reel/aBcDeF1234/",
            SocialKind::Reel,
            None,
        ),
        (
            "https://www.instagram.com/p/aBcDeF1234/",
            SocialKind::Post,
            None,
        ),
    ] {
        let want = if text.contains("instagram") {
            "Instagram"
        } else {
            "Facebook"
        };
        assert_eq!(
            site(text),
            Some(format!("{want} · {}", kind.label().unwrap())),
            "{text}"
        );
        assert!(!has_path(text), "{text}");
        let card = parse(text).unwrap();
        assert_eq!(card.video_id.as_deref(), id, "{text}");
        let shown = card.text();
        assert!(!shown.contains("someone"), "{shown}");
        match id {
            Some(id) => assert!(
                shown.contains(&format!("{} · {id}", site(text).unwrap())),
                "{shown}"
            ),
            None => {
                for id in [NUM, "aBcDeF1234"] {
                    assert!(!shown.contains(id), "{id} in {shown}");
                }
            }
        }
        assert!(chips(text).is_empty(), "{text}: {:?}", chips(text));
    }
    // Only the strict shape: digits for a Facebook number, letters and digits for fb.watch.
    for odd in [
        "https://www.facebook.com/reel/10923a4577040057",
        "https://www.facebook.com/reel/1234567",
        "https://www.facebook.com/watch?v=abc",
        "https://www.facebook.com/someone/videos/123456789012345678901",
        "https://fb.watch/ab-c_d1234/",
        "https://fb.watch/abc/",
    ] {
        assert_eq!(parse(odd).unwrap().video_id, None, "{odd}");
    }
    // Other parameters keep their raw chip; a profile page keeps its path.
    assert_eq!(
        chips("https://www.facebook.com/reel/1092344577040057?mibextid=x"),
        ["mibextid"]
    );
    assert!(has_path("https://www.facebook.com/someone"));
    assert_eq!(site("https://www.facebook.com/someone"), None);
}

#[test]
fn tiktok_links_say_what_they_are() {
    let site = |text: &str| parse(text).unwrap().site.clone();
    let video = "https://www.tiktok.com/@someone/video/7312345678901234567?lang=nl";
    assert_eq!(site(video).as_deref(), Some("TikTok · Video"));
    assert!(!has_path(video));
    // The video number is shown, the user never.
    let card = parse(video).unwrap();
    assert_eq!(card.video_id.as_deref(), Some("7312345678901234567"));
    let shown = card.text();
    assert!(
        shown.contains("TikTok · Video · 7312345678901234567"),
        "{shown}"
    );
    assert!(!shown.contains("someone"), "{shown}");
    assert!(!card.ax_label().contains("someone"));
    assert_eq!(chips(video), ["lang"]);
    let picture = parse("https://www.tiktok.com/@someone/photo/7312345678901234568").unwrap();
    assert_eq!(picture.video_id.as_deref(), Some("7312345678901234568"));
    assert!(!picture.text().contains("someone"));
    assert_eq!(
        site("https://vm.tiktok.com/ZMabc123/").as_deref(),
        Some("TikTok · Video")
    );
    assert_eq!(
        parse("https://vm.tiktok.com/ZMabc123/")
            .unwrap()
            .video_id
            .as_deref(),
        Some("ZMabc123")
    );
    assert_eq!(
        parse("https://vt.tiktok.com/ZSabc123/")
            .unwrap()
            .video_id
            .as_deref(),
        Some("ZSabc123")
    );
    // Only the strict shape: 15 to 20 digits, a short code of letters and digits.
    for odd in [
        "https://www.tiktok.com/@someone/video/73123456789x1234567",
        "https://www.tiktok.com/@someone/video/12345",
        "https://www.tiktok.com/@someone/video/731234567890123456789",
        "https://vm.tiktok.com/ZM-abc_12/",
        "https://vm.tiktok.com/ab/",
    ] {
        let card = parse(odd).unwrap();
        assert_eq!(card.video_id, None, "{odd}");
        assert!(!card.text().contains("someone"), "{odd}");
    }
    // A profile or a hashtag shows no ID.
    assert_eq!(
        parse("https://www.tiktok.com/@someone").unwrap().video_id,
        None
    );
    assert_eq!(
        parse("https://www.tiktok.com/tag/cats").unwrap().video_id,
        None
    );
    for start in [
        "https://www.tiktok.com/nl-NL/",
        "https://www.tiktok.com/",
        "https://m.tiktok.com/en",
    ] {
        assert_eq!(site(start).as_deref(), Some("TikTok"), "{start}");
        assert!(!has_path(start), "{start}");
    }
    let profile = "https://www.tiktok.com/@someone";
    assert_eq!(
        site(profile),
        Some(format!(
            "TikTok · {}",
            crate::locale::t("link_social_profile")
        ))
    );
    assert!(has_path(profile));
    assert_eq!(
        site("https://www.tiktok.com/tag/cats"),
        Some(format!(
            "TikTok · {}",
            crate::locale::t("link_social_hashtag")
        ))
    );
    assert_eq!(site("https://www.tiktok.com/some/other/page"), None);
}

#[test]
fn youtube_facebook_and_tiktok_get_their_own_symbol() {
    use super::Brand;
    let symbol = |text: &str| parse(text).unwrap().symbol();
    assert_eq!(
        symbol("https://www.youtube.com/watch?v=aBcDeFgHiJk"),
        "play.rectangle.fill"
    );
    assert_eq!(
        symbol("https://youtu.be/aBcDeFgHiJk"),
        "play.rectangle.fill"
    );
    assert_eq!(
        symbol("https://www.facebook.com/reel/1092344577040057"),
        "f.square.fill"
    );
    assert_eq!(symbol("https://fb.watch/aBcDeF1234/"), "f.square.fill");
    assert_eq!(symbol("https://www.tiktok.com/nl-NL/"), "music.note");
    assert_eq!(symbol("https://vm.tiktok.com/ZMabc123/"), "music.note");
    for other in [
        "https://www.instagram.com/p/aBcDeF1234/",
        "https://github.com/acme/app",
        "https://example.com/",
    ] {
        assert_eq!(symbol(other), "globe", "{other}");
    }
    assert_eq!(symbol("http://localhost:3000/"), "network");
    assert_eq!(
        Brand::YouTube.tint(),
        crate::card_theme::Rgb(0xFF, 0x00, 0x00)
    );
    assert_eq!(
        Brand::Facebook.tint(),
        crate::card_theme::Rgb(0x18, 0x77, 0xF2)
    );
    assert_eq!(
        Brand::TikTok.tint(),
        crate::card_theme::Rgb(0xFE, 0x2C, 0x55)
    );
    for brand in [Brand::YouTube, Brand::Facebook, Brand::TikTok] {
        assert_eq!(Brand::of_symbol(brand.symbol()), Some(brand));
    }
    assert_eq!(Brand::of_symbol("globe"), None);
}
