//! What the parts of a known site's link mean, for the Link card: `/watch?v=…` on YouTube is a
//! video ID, `/document/d/…/edit` on Google Docs a document ID. A part that is explained this
//! way is shown as its readable name (*Video-ID*), not as the raw path or parameter name
//! (`/watch`, `v`). Names and path shapes only: values and IDs are never read into the
//! result, and nothing is logged.

/// A readable name for one part of a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Meaning {
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
}

impl Meaning {
    /// The name on the chip, in the app's language.
    pub(crate) fn label(self) -> &'static str {
        use crate::locale::t;
        match self {
            Meaning::VideoId => t("link_mean_video_id"),
            Meaning::Short => t("link_mean_short"),
            Meaning::StartTime => t("link_mean_start_time"),
            Meaning::Playlist => t("link_mean_playlist"),
            Meaning::PlaylistPosition => t("link_mean_playlist_position"),
            Meaning::Tab => t("link_mean_tab"),
            Meaning::SearchTerms => t("link_mean_search"),
            Meaning::DocumentId => t("link_mean_document_id"),
            Meaning::SheetTab => t("link_mean_sheet_tab"),
            Meaning::FileId => t("link_mean_file_id"),
            Meaning::FolderId => t("link_mean_folder_id"),
            Meaning::SelectedLayer => t("link_mean_selected_layer"),
            Meaning::QuestionNumber => t("link_mean_question_number"),
            Meaning::SelectedIssue => t("link_mean_selected_issue"),
            Meaning::PageId => t("link_mean_page_id"),
            Meaning::DatabaseView => t("link_mean_database_view"),
        }
    }
}

/// What a known site's link explains.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Explained {
    /// The path says nothing the site line and the meanings do not: hide it.
    pub path: bool,
    /// Readable names, each once, in order (the path's first, then the parameters').
    pub meanings: Vec<Meaning>,
    /// Parameter names a meaning stands for: no raw chip for them.
    pub params: Vec<String>,
}

impl Explained {
    fn mean(&mut self, meaning: Meaning) {
        if !self.meanings.contains(&meaning) {
            self.meanings.push(meaning);
        }
    }
}

const YOUTUBE_HOSTS: &[&str] = &[
    "youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "youtube-nocookie.com",
];

/// The parts of a link on `host` (lower case, without `www.`) with these decoded path
/// `segments` and parameter `names` that a known site explains. Empty for any other host.
pub(crate) fn explain(host: &str, segments: &[String], names: &[String]) -> Explained {
    let seg = |at: usize| segments.get(at).map(String::as_str);
    let has = |name: &str| names.iter().any(|n| n == name);
    let mut out = Explained::default();
    // (parameter name, meaning) pairs this site explains.
    let mut known: Vec<(&str, Meaning)> = Vec::new();
    if host == "youtu.be" {
        if segments.len() == 1 {
            out.path = true;
            out.mean(Meaning::VideoId);
        }
        known = vec![("t", Meaning::StartTime), ("list", Meaning::Playlist)];
    } else if YOUTUBE_HOSTS.contains(&host) {
        match (seg(0), segments.len()) {
            (Some("watch"), 1) if has("v") => out.path = true,
            (Some("playlist"), 1) if has("list") => out.path = true,
            (Some("shorts"), 2) => {
                out.path = true;
                out.mean(Meaning::Short);
                out.mean(Meaning::VideoId);
            }
            (Some("embed" | "live" | "v"), 2) => {
                out.path = true;
                out.mean(Meaning::VideoId);
            }
            _ => {}
        }
        known = vec![
            ("v", Meaning::VideoId),
            ("t", Meaning::StartTime),
            ("start", Meaning::StartTime),
            ("list", Meaning::Playlist),
            ("index", Meaning::PlaylistPosition),
        ];
        if seg(0) == Some("results") {
            known.push(("search_query", Meaning::SearchTerms));
        }
    } else if host == "github.com" {
        // The site line holds owner/repo and the PR, issue or commit.
        out.path = match (seg(2), segments.len()) {
            (None, 1 | 2) => true,
            (Some("pull" | "issues"), 4) => seg(3).is_some_and(is_number),
            (Some("commit"), 4) => seg(3).is_some_and(is_hex7),
            (Some("pulls" | "issues"), 3) => true,
            _ => false,
        };
        known = vec![("tab", Meaning::Tab)];
        if matches!(seg(2), Some("pulls" | "issues")) || segments.is_empty() {
            known.push(("q", Meaning::SearchTerms));
        }
    } else if let Some(kind) = social_kind(host, segments, names) {
        // Facebook, Instagram and TikTok: the site line says what it is; the IDs say nothing.
        // A profile or hashtag keeps its path (the name, not an ID).
        out.path = !matches!(kind, SocialKind::Profile | SocialKind::Hashtag);
        for name in names {
            if SOCIAL_ID_PARAMS.contains(&name.as_str()) {
                out.params.push(name.clone());
            }
        }
    } else if host == "gitlab.com" || host.starts_with("gitlab.") {
        if let Some(dash) = segments.iter().position(|s| s == "-") {
            let after = |at: usize| segments.get(dash + 1 + at).map(String::as_str);
            out.path = segments.len() == dash + 3
                && match after(0) {
                    Some("merge_requests" | "issues") => after(1).is_some_and(is_number),
                    Some("commit") => after(1).is_some_and(is_hex7),
                    _ => false,
                };
        }
    } else if host.ends_with(".atlassian.net") || host.split('.').any(|label| label == "jira") {
        if seg(0) == Some("browse") && segments.len() == 2 {
            out.path = true;
        }
        known = vec![("selectedIssue", Meaning::SelectedIssue)];
    } else if host == "notion.so" || host.ends_with(".notion.so") || host.ends_with(".notion.site")
    {
        // `/Page-Title-<32 hex>` or `/workspace/Page-Title-<32 hex>`; the site line has the title.
        if (1..=2).contains(&segments.len())
            && segments.last().is_some_and(|last| {
                let id = last.rsplit_once('-').map_or(last.as_str(), |(_, id)| id);
                id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit())
            })
        {
            out.path = true;
            out.mean(Meaning::PageId);
        }
        known = vec![("v", Meaning::DatabaseView)];
    } else if host == "docs.google.com" {
        // `/document/d/<id>`, then `edit`, `view`, … or nothing.
        if matches!(
            seg(0),
            Some("document" | "spreadsheets" | "presentation" | "forms" | "drawings")
        ) && seg(1) == Some("d")
            && seg(2).is_some()
            && segments.len() <= 4
        {
            out.path = true;
            out.mean(Meaning::DocumentId);
        }
        if seg(0) == Some("spreadsheets") {
            known = vec![("gid", Meaning::SheetTab)];
        }
    } else if host == "drive.google.com" {
        match (seg(0), seg(1), seg(2)) {
            (Some("file"), Some("d"), Some(_)) if segments.len() <= 4 => {
                out.path = true;
                out.mean(Meaning::FileId);
            }
            (Some("drive"), Some("folders"), Some(_)) if segments.len() == 3 => {
                out.path = true;
                out.mean(Meaning::FolderId);
            }
            _ => {}
        }
        if seg(0) == Some("open") && segments.len() == 1 {
            out.path = true;
            known = vec![("id", Meaning::FileId)];
        }
    } else if host == "figma.com" {
        // `/design/<key>/<name>`: the site line has the name.
        if matches!(
            seg(0),
            Some("file" | "design" | "proto" | "board" | "slides")
        ) && seg(1).is_some()
            && segments.len() <= 3
        {
            out.path = true;
            out.mean(Meaning::FileId);
        }
        known = vec![("node-id", Meaning::SelectedLayer)];
    } else if host == "stackoverflow.com"
        && seg(0) == Some("questions")
        && seg(1).is_some_and(is_number)
        && segments.len() <= 3
    {
        out.path = true;
        out.mean(Meaning::QuestionNumber);
    }
    for name in names {
        if out.params.contains(name) {
            continue;
        }
        if let Some((_, meaning)) = known.iter().find(|(known, _)| known == name) {
            out.mean(*meaning);
            out.params.push(name.clone());
        }
    }
    out
}

/// What a Facebook, Instagram or TikTok link is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SocialKind {
    Reel,
    Video,
    Post,
    Group,
    Event,
    Photo,
    Profile,
    Hashtag,
    /// The site's start page: the site line is the site's name alone.
    StartPage,
}

impl SocialKind {
    /// What follows the site's name on the site line; `None` for the start page.
    pub(crate) fn label(self) -> Option<&'static str> {
        use crate::locale::t;
        Some(match self {
            SocialKind::Reel => t("link_social_reel"),
            SocialKind::Video => t("link_social_video"),
            SocialKind::Post => t("link_social_post"),
            SocialKind::Group => t("link_social_group"),
            SocialKind::Event => t("link_social_event"),
            SocialKind::Photo => t("link_social_photo"),
            SocialKind::Profile => t("link_social_profile"),
            SocialKind::Hashtag => t("link_social_hashtag"),
            SocialKind::StartPage => return None,
        })
    }
}

/// Parameters of a Facebook link that only hold an ID: no raw chip.
const SOCIAL_ID_PARAMS: &[&str] = &["v", "fbid", "set", "story_fbid", "id"];

const FACEBOOK_HOSTS: &[&str] = &["facebook.com", "m.facebook.com", "web.facebook.com"];

const TIKTOK_HOSTS: &[&str] = &["tiktok.com", "m.tiktok.com"];
/// TikTok's short links (`vm.tiktok.com/<code>`): always a video.
const TIKTOK_SHORT_HOSTS: &[&str] = &["vm.tiktok.com", "vt.tiktok.com"];

/// A site with its own symbol and colour on the Link card and the strip tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Brand {
    YouTube,
    Facebook,
    TikTok,
}

impl Brand {
    /// The SF Symbol (no logo is bundled), tinted [`Self::tint`].
    pub(crate) fn symbol(self) -> &'static str {
        match self {
            Brand::YouTube => "play.rectangle.fill",
            Brand::Facebook => "f.square.fill",
            Brand::TikTok => "music.note",
        }
    }

    /// YouTube red, Facebook blue, TikTok pink.
    pub(crate) fn tint(self) -> crate::card_theme::Rgb {
        use crate::card_theme::Rgb;
        match self {
            Brand::YouTube => Rgb(0xFF, 0x00, 0x00),
            Brand::Facebook => Rgb(0x18, 0x77, 0xF2),
            Brand::TikTok => Rgb(0xFE, 0x2C, 0x55),
        }
    }

    /// The brand whose [`Self::symbol`] is `symbol`.
    pub(crate) fn of_symbol(symbol: &str) -> Option<Brand> {
        [Brand::YouTube, Brand::Facebook, Brand::TikTok]
            .into_iter()
            .find(|brand| brand.symbol() == symbol)
    }
}

/// The brand of `host` (lower case, without `www.`).
pub(crate) fn brand(host: &str) -> Option<Brand> {
    if host == "youtu.be" || YOUTUBE_HOSTS.contains(&host) {
        Some(Brand::YouTube)
    } else if host == "fb.watch" || FACEBOOK_HOSTS.contains(&host) {
        Some(Brand::Facebook)
    } else if TIKTOK_HOSTS.contains(&host) || TIKTOK_SHORT_HOSTS.contains(&host) {
        Some(Brand::TikTok)
    } else {
        None
    }
}

/// The site and kind of a Facebook, Instagram or TikTok link with a known shape; `None`
/// otherwise.
pub(crate) fn social(host: &str, segments: &[String], names: &[String]) -> Option<String> {
    let kind = social_kind(host, segments, names)?;
    let site = if host.contains("instagram") {
        "Instagram"
    } else if host.contains("tiktok") {
        "TikTok"
    } else {
        "Facebook"
    };
    Some(match kind.label() {
        Some(label) => format!("{site} · {label}"),
        None => site.to_string(),
    })
}

/// `nl-NL`, `en`, `pt-BR`: a language path on TikTok's start page.
fn is_language(segment: &str) -> bool {
    let (lang, region) = segment.split_once('-').unwrap_or((segment, ""));
    lang.len() == 2
        && lang.chars().all(|c| c.is_ascii_lowercase())
        && (region.is_empty()
            || (region.len() == 2 && region.chars().all(|c| c.is_ascii_alphabetic())))
}

fn tiktok_kind(host: &str, segments: &[String]) -> Option<SocialKind> {
    let seg = |at: usize| segments.get(at).map(String::as_str);
    let len = segments.len();
    if TIKTOK_SHORT_HOSTS.contains(&host) {
        return match len {
            0 => Some(SocialKind::StartPage),
            1 => Some(SocialKind::Video),
            _ => None,
        };
    }
    if !TIKTOK_HOSTS.contains(&host) {
        return None;
    }
    let user = |at: usize| seg(at).is_some_and(|s| s.len() > 1 && s.starts_with('@'));
    match (seg(0), seg(1), len) {
        (None, _, 0) => Some(SocialKind::StartPage),
        (Some(lang), None, 1) if is_language(lang) => Some(SocialKind::StartPage),
        (_, Some("video"), 3) if user(0) => Some(SocialKind::Video),
        (_, Some("photo"), 3) if user(0) => Some(SocialKind::Photo),
        (_, None, 1) if user(0) => Some(SocialKind::Profile),
        (Some("tag"), Some(_), 2) => Some(SocialKind::Hashtag),
        _ => None,
    }
}

fn social_kind(host: &str, segments: &[String], names: &[String]) -> Option<SocialKind> {
    let seg = |at: usize| segments.get(at).map(String::as_str);
    let has = |name: &str| names.iter().any(|n| n == name);
    let len = segments.len();
    if host.contains("tiktok") {
        return tiktok_kind(host, segments);
    }
    if host == "fb.watch" {
        return (len == 1).then_some(SocialKind::Video);
    }
    if host == "instagram.com" || host == "m.instagram.com" {
        return match (seg(0), len) {
            (Some("reel" | "reels"), 2) => Some(SocialKind::Reel),
            (Some("p"), 2) => Some(SocialKind::Post),
            (Some("tv"), 2) => Some(SocialKind::Video),
            _ => None,
        };
    }
    if !FACEBOOK_HOSTS.contains(&host) {
        return None;
    }
    match (seg(0), seg(1), len) {
        (Some("reel"), Some(_), 2) => Some(SocialKind::Reel),
        (Some("watch"), None, 1) if has("v") => Some(SocialKind::Video),
        (Some("groups"), Some(_), 2) => Some(SocialKind::Group),
        (Some("events"), Some(_), 2) => Some(SocialKind::Event),
        (Some("photo" | "photo.php"), None, 1) => Some(SocialKind::Photo),
        (Some("story.php" | "permalink.php"), None, 1) => Some(SocialKind::Post),
        (Some(_), Some("posts"), 3) => Some(SocialKind::Post),
        (Some(_), Some("videos"), 3) => Some(SocialKind::Video),
        _ => None,
    }
}

/// The ID of a TikTok or Facebook video link, when it has the strict shape: digits only for
/// TikTok (15-20) and Facebook (8-20) video numbers, letters and digits for a TikTok
/// (`vm.`/`vt.`, 5-12) or `fb.watch` (5-15) short code. Users, posts, groups, events and
/// pictures give `None`, like Instagram and every other site.
pub(crate) fn social_video_id(
    host: &str,
    segments: &[String],
    pairs: &[(String, String)],
) -> Option<String> {
    let seg = |at: usize| segments.get(at).map(String::as_str);
    let digits =
        |text: &str, min: usize, max: usize| (min..=max).contains(&text.len()) && is_number(text);
    let code = |text: &str, min: usize, max: usize| {
        (min..=max).contains(&text.len()) && text.chars().all(|c| c.is_ascii_alphanumeric())
    };
    let names: Vec<String> = pairs.iter().map(|(name, _)| name.clone()).collect();
    let kind = social_kind(host, segments, &names)?;
    let id = if TIKTOK_SHORT_HOSTS.contains(&host) {
        seg(0).filter(|s| code(s, 5, 12))
    } else if TIKTOK_HOSTS.contains(&host) {
        match kind {
            SocialKind::Video | SocialKind::Photo => seg(2).filter(|s| digits(s, 15, 20)),
            _ => None,
        }
    } else if host == "fb.watch" {
        seg(0).filter(|s| code(s, 5, 15))
    } else if FACEBOOK_HOSTS.contains(&host) {
        match (kind, seg(0)) {
            (SocialKind::Reel, _) => seg(1),
            (SocialKind::Video, Some("watch")) => pairs
                .iter()
                .find(|(name, _)| name == "v")
                .map(|(_, value)| value.as_str()),
            (SocialKind::Video, _) => seg(2),
            _ => None,
        }
        .filter(|s| digits(s, 8, 20))
    } else {
        None
    };
    id.map(str::to_string)
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
}

fn is_hex7(text: &str) -> bool {
    text.len() >= 7 && text.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "link_meaning_tests.rs"]
mod tests;
