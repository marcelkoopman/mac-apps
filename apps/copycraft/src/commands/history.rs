//! History, versions, undo and the overflow menu.

use super::*;

/// The version capsule in the chip row, left of the history capsule: shown in the Dataframe
/// view while the table has more than one version. Text and pictures have none: a Text ▾ or
/// Image ▾ step makes a new copy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VersionBar {
    /// "Original", then each step's label.
    pub labels: Vec<String>,
    /// The version shown.
    pub current: usize,
}

impl VersionBar {
    pub fn of(data: &LaunchData) -> Option<Self> {
        if data.subject_kind == SubjectKind::Image {
            return None;
        }
        let table = data.table.as_ref()?;
        let text = data.subject_text.as_deref()?;
        (presented_view(text, data.view) == CardView::Dataframe && table.labels.len() > 1).then(
            || Self {
                labels: table.labels.clone(),
                current: table.version.min(table.labels.len() - 1),
            },
        )
    }

    /// "v2/3", the capsule's label.
    pub fn title(&self) -> String {
        format!("v{}/{}", self.current + 1, self.labels.len())
    }

    /// "Version 2 of 3, Duplicates removed": the capsule's tooltip and VoiceOver label.
    pub fn spoken(&self) -> String {
        crate::locale::tf(
            "version_spoken",
            &[
                &(self.current + 1),
                &self.labels.len(),
                &self.labels[self.current],
            ],
        )
    }

    pub fn can_undo(&self) -> bool {
        self.current > 0
    }

    pub fn can_redo(&self) -> bool {
        self.current + 1 < self.labels.len()
    }

    /// The versions to pick from, the one shown first-class (`true`).
    pub fn menu(&self) -> Vec<(Command, bool)> {
        self.labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let title = format!("{}. {label}", index + 1);
                (
                    command(
                        CommandId::TableVersion(index),
                        &title,
                        crate::locale::t("version"),
                        "version",
                    ),
                    index == self.current,
                )
            })
            .collect()
    }
}

/// What ⌘Z (`shift`: ⇧⌘Z) does on the card: undo or redo a table step when there is one, else
/// nothing (`None`, so a text field keeps the key). `chars` is the key without modifiers.
pub fn undo_key(
    chars: &str,
    command: bool,
    shift: bool,
    other_modifier: bool,
    bar: Option<&VersionBar>,
) -> Option<CommandId> {
    let bar = bar?;
    step_key(
        chars,
        command,
        shift,
        other_modifier,
        bar.can_undo(),
        bar.can_redo(),
    )
}

/// ⌘Z and ⇧⌘Z on a picture card after an Image ▾ step: the copy it was taken on, and back
/// (`edit`'s [`ImageShown::can_undo`] and `can_redo`), without a capsule. As [`undo_key`].
pub fn image_undo_key(
    chars: &str,
    command: bool,
    shift: bool,
    other_modifier: bool,
    edit: Option<&ImageShown>,
) -> Option<CommandId> {
    let edit = edit?;
    step_key(
        chars,
        command,
        shift,
        other_modifier,
        edit.can_undo,
        edit.can_redo,
    )
}

/// ⌘Z and ⇧⌘Z on a text card after a Text ▾ step: as [`image_undo_key`], with `edit`'s
/// [`TextShown::can_undo`] and `can_redo`.
pub fn text_undo_key(
    chars: &str,
    command: bool,
    shift: bool,
    other_modifier: bool,
    edit: Option<&TextShown>,
) -> Option<CommandId> {
    let edit = edit?;
    step_key(
        chars,
        command,
        shift,
        other_modifier,
        edit.can_undo,
        edit.can_redo,
    )
}

fn step_key(
    chars: &str,
    command: bool,
    shift: bool,
    other_modifier: bool,
    can_undo: bool,
    can_redo: bool,
) -> Option<CommandId> {
    if !command || other_modifier || !chars.eq_ignore_ascii_case("z") {
        return None;
    }
    if shift {
        can_redo.then_some(CommandId::StepForward)
    } else {
        can_undo.then_some(CommandId::StepBack)
    }
}

/// ⌘Z right after a copy was taken out of history (`can_restore`) puts it back
/// ([`CommandId::RestoreHistory`]). It goes before a table's undo; ⇧⌘Z is the table's.
pub fn restore_key(
    chars: &str,
    command: bool,
    shift: bool,
    other_modifier: bool,
    can_restore: bool,
) -> Option<CommandId> {
    (can_restore && command && !shift && !other_modifier && chars.eq_ignore_ascii_case("z"))
        .then_some(CommandId::RestoreHistory)
}

/// ⌫ on the card takes the copy the strip marks (history position `cursor`) out of history.
/// Not while the search field is open (it edits the search), and only while the strip shows.
pub fn delete_key(searching: bool, strip_shown: bool, cursor: usize) -> Option<CommandId> {
    (!searching && strip_shown).then_some(CommandId::RemoveHistory(cursor))
}

/// The `⋯` row of a [`CommandId::KeepHistory`] choice.
pub fn keep_history_title(minutes: u32) -> String {
    if minutes == 0 {
        crate::locale::t("keep_history_forever").to_string()
    } else {
        crate::locale::tf("keep_history_minutes", &[&minutes])
    }
}

pub fn matching(commands: &[Command], query: &str) -> Vec<Command> {
    let mut needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let found: Vec<Command> = commands
        .iter()
        .filter(|cmd| cmd.matches(&needle))
        .take(MAX_VISIBLE)
        .cloned()
        .collect();
    needle.zeroize();
    found
}

/// Title of a history hit. With the privacy filter on, or for a picture, this is the type
/// and size (and the pin mark). With the filter off it is one line of the text, cut at about
/// 80 characters. The match can sit outside that line: keywords hold the capped text.
pub fn history_chip_title(item: &Hist, privacy: bool) -> String {
    if privacy || item.search.is_empty() {
        return item.title.clone();
    }
    const EXCERPT: usize = 80;
    let line = item.search.lines().next().unwrap_or("").trim();
    let mut end = EXCERPT.min(line.len());
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    let mut title = line[..end].to_string();
    if line.len() > end {
        title.push('…');
    }
    title
}

pub(crate) fn history_keywords(item: &Hist) -> String {
    let mut keywords = String::from("history");
    if !item.search.is_empty() {
        keywords.push(' ');
        keywords.push_str(item.search.as_str());
    }
    keywords
}

/// Pin, unpin and remove, for the entry on screen.
pub(crate) fn entry_commands(data: &LaunchData) -> Vec<Command> {
    let Some(shown) = shown_history(data) else {
        return Vec::new();
    };
    let pin = if shown.pinned {
        command(
            CommandId::Unpin,
            crate::locale::t("unpin"),
            crate::locale::t("detail_unpin"),
            "unpin pin history",
        )
    } else {
        command(
            CommandId::Pin,
            crate::locale::t("pin"),
            crate::locale::t("detail_pin"),
            "pin history",
        )
    };
    vec![
        pin,
        command(
            CommandId::DeleteHistory,
            crate::locale::t("remove_history"),
            crate::locale::t("detail_remove_history"),
            "delete remove history",
        ),
    ]
}

/// Pause, ignore-next and ignore-frontmost. Quit stays out of search. Greyed out while
/// "Keep clipboard history" is off: then no copy is read on its own, so there is nothing to hold.
pub(crate) fn capture_commands(data: &LaunchData) -> Vec<Command> {
    let active = crate::history_mode::capture_controls_active(data.settings.clipboard_history);
    vec![
        command_enabled(
            CommandId::Pause,
            crate::locale::t("pause_recording"),
            crate::locale::t("detail_pause"),
            "pause recording history",
            active,
        ),
        command_enabled(
            CommandId::IgnoreNext,
            crate::locale::t("ignore_next"),
            crate::locale::t("detail_ignore_next"),
            "ignore next copy",
            active,
        ),
        command_enabled(
            CommandId::IgnoreFrontApp,
            crate::locale::t("ignore_front_app"),
            crate::locale::t("detail_ignore_app"),
            "ignore app frontmost",
            active && data.can_ignore_app,
        ),
    ]
}

/// *Take Screenshot…*, in search. Greyed out, with a note why, where ScreenCaptureKit is
/// missing.
pub(crate) fn screenshot_command(data: &LaunchData) -> Command {
    let detail = if data.can_screenshot {
        crate::locale::t("detail_screenshot")
    } else {
        crate::locale::t("screenshot_unavailable")
    };
    command_enabled(
        CommandId::TakeScreenshot,
        crate::locale::t("take_screenshot"),
        detail,
        "screenshot capture screen window area schermafbeelding scherm",
        data.can_screenshot,
    )
}

/// The history entry on screen, when the card is showing one.
pub(crate) fn shown_history(data: &LaunchData) -> Option<&Hist> {
    let id = data.shown_id?;
    data.history.iter().find(|item| item.id == id)
}

pub(crate) fn diff_swap_command() -> Command {
    command(
        CommandId::DiffSwap,
        crate::locale::t("diff_swap"),
        crate::locale::t("detail_diff_swap"),
        "diff swap",
    )
}

/// "Voeg toe aan diff", "Vergelijk met A", or "Verwijder uit diff" for entry `entry`.
pub(crate) fn diff_action(id: CommandId, picks: &[u64], entry: u64, is_text: bool) -> Command {
    let menu = crate::diff::menu_for(picks, entry, is_text);
    let (title, detail) = match menu.kind {
        crate::diff::MenuKind::Add => (
            crate::locale::t("diff_add"),
            crate::locale::t("detail_diff"),
        ),
        crate::diff::MenuKind::Compare => (
            crate::locale::t("diff_compare"),
            crate::locale::t("detail_diff"),
        ),
        crate::diff::MenuKind::Remove => (
            crate::locale::t("diff_remove"),
            crate::locale::t("detail_diff"),
        ),
    };
    command_enabled(id, title, detail, "diff compare", menu.enabled)
}

/// Quit, earlier copies, and housekeeping. Copies stay out of [`chips`].
pub fn overflow(data: &LaunchData) -> Vec<Command> {
    let mut commands = source_commands(data);
    commands.push(command(
        CommandId::ClearClipboard,
        crate::locale::t("empty_pasteboard"),
        crate::locale::t("detail_pasteboard_only"),
        "empty pasteboard clear",
    ));
    // The copy on screen, above History, so the action is visible while stepping with the arrows.
    if let Some(shown) = shown_history(data) {
        commands.push(diff_action(
            CommandId::DiffShown,
            &data.diff_picks,
            shown.id,
            shown.text,
        ));
    }
    if data.diff_picks.len() == 2 {
        commands.push(diff_swap_command());
    }
    commands.extend(entry_commands(data));
    for item in &data.history {
        let title = match crate::diff::side_label(&data.diff_picks, item.id) {
            Some(mark) => format!("{mark}  {}", item.title),
            None => item.title.clone(),
        };
        commands.push(Command {
            pinned: item.pinned,
            ..command(
                CommandId::History(item.index),
                &title,
                &item.mark,
                "history",
            )
        });
        commands.push(diff_action(
            CommandId::DiffToggle(item.id),
            &data.diff_picks,
            item.id,
            item.text,
        ));
    }
    if data.can_clear_history {
        commands.push(command(
            CommandId::ClearHistory,
            crate::locale::t("clear_history"),
            crate::locale::t("detail_forget_copies"),
            "clear history forget",
        ));
    }
    if data.can_clear_pins {
        commands.push(command(
            CommandId::ClearPins,
            crate::locale::t("clear_pins"),
            crate::locale::t("detail_clear_pins"),
            "clear pins history",
        ));
    }
    for minutes in crate::settings::HISTORY_MINUTES {
        commands.push(command(
            CommandId::KeepHistory(minutes),
            &keep_history_title(minutes),
            crate::locale::t("detail_history_forgotten"),
            "keep history forget minutes",
        ));
    }
    commands.extend(capture_commands(data));
    commands.push(command(
        CommandId::ClearSensitive,
        crate::locale::t("clear_sensitive"),
        crate::locale::t("detail_clear_sensitive"),
        "clear sensitive pasteboard timer",
    ));
    commands.push(command(
        CommandId::ToggleStrip,
        strip_toggle_title(strip_turned_off()),
        crate::locale::t("strip_group"),
        "history strip thumbnails cards historie strook",
    ));
    // Theme ▸: the card themes in Settings' order, a separator between their groups. The app
    // appearance (System, Light, Dark) stays in command search.
    for theme in crate::card_theme::CardTheme::ALL {
        commands.push(command(
            CommandId::CardTheme(theme),
            theme.title(),
            crate::locale::t("theme_menu"),
            "theme card colours",
        ));
    }
    for size in crate::settings::WindowSize::ALL {
        commands.push(command(
            CommandId::WindowSize(size),
            window_size_title(size),
            crate::locale::t("window_size_menu"),
            "window size width wide compact normal resize venstergrootte breed",
        ));
    }
    commands.push(command(
        CommandId::Settings,
        crate::locale::t("settings"),
        crate::locale::t("detail_settings"),
        "settings preferences hotkey privacy filter blur login ignored apps",
    ));
    commands.push(command(
        CommandId::About,
        crate::locale::t("about"),
        crate::locale::t("detail_about"),
        "about version offline",
    ));
    // Help ▸.
    commands.push(command(
        CommandId::Welcome,
        crate::locale::t("welcome_menu"),
        crate::locale::t("detail_welcome"),
        "welcome start introduction",
    ));
    commands.push(command(
        CommandId::SuggestFeature,
        crate::locale::t("feedback_feature_menu"),
        crate::locale::t("detail_feedback_feature"),
        "feature suggest suggestion idea feedback voorstel idee mail",
    ));
    commands.push(command(
        CommandId::ReportProblem,
        crate::locale::t("feedback_problem_menu"),
        crate::locale::t("detail_feedback_problem"),
        "problem report bug issue feedback probleem melden fout mail",
    ));
    commands.push(command(
        CommandId::ExportDiagnostics,
        crate::locale::t("export_diagnostics"),
        crate::locale::t("detail_export_diagnostics"),
        "export diagnostics log support diagnose",
    ));
    commands.push(command(
        CommandId::CheckAi,
        crate::locale::t("ai_check"),
        crate::locale::t("detail_ai_check"),
        "ai apple intelligence check controleer status health gezondheid",
    ));
    commands.push(command(
        CommandId::Quit,
        crate::locale::t("quit"),
        crate::locale::t("detail_quit"),
        "quit exit",
    ));
    commands
}

/// The sections of the `⋯` menu, in order. A header (or a separator before macOS 14) goes
/// between two sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MenuSection {
    /// Chips that did not fit on the card.
    More,
    File,
    Clipboard,
    History,
    Recording,
    Appearance,
    App,
}

impl MenuSection {
    /// The header title, NL/EN; `None` for the chips that did not fit.
    pub fn title(self) -> Option<&'static str> {
        match self {
            MenuSection::More => None,
            MenuSection::File => Some(crate::locale::t("section_file")),
            MenuSection::Clipboard => Some(crate::locale::t("section_clipboard")),
            MenuSection::History => Some(crate::locale::t("history")),
            MenuSection::Recording => Some(crate::locale::t("section_recording")),
            MenuSection::Appearance => Some(crate::locale::t("appearance")),
            MenuSection::App => Some("CopyCraft"),
        }
    }
}

/// Which section of the `⋯` menu `id` belongs to.
pub fn menu_section(id: &CommandId) -> MenuSection {
    match id {
        CommandId::ChooseFile | CommandId::Generate(_) | CommandId::UseClipboard => {
            MenuSection::File
        }
        CommandId::ClearClipboard
        | CommandId::DiffShown
        | CommandId::DiffSwap
        | CommandId::Pin
        | CommandId::Unpin
        | CommandId::DeleteHistory => MenuSection::Clipboard,
        CommandId::History(_)
        | CommandId::DiffToggle(_)
        | CommandId::ClearHistory
        | CommandId::ClearPins
        | CommandId::KeepHistory(_) => MenuSection::History,
        CommandId::Pause
        | CommandId::IgnoreNext
        | CommandId::IgnoreFrontApp
        | CommandId::ClearSensitive => MenuSection::Recording,
        CommandId::Appearance(_)
        | CommandId::CardTheme(_)
        | CommandId::WindowSize(_)
        | CommandId::ToggleStrip => MenuSection::Appearance,
        CommandId::Settings
        | CommandId::About
        | CommandId::SuggestFeature
        | CommandId::ReportProblem
        | CommandId::ExportDiagnostics
        | CommandId::CheckAi
        | CommandId::Welcome
        | CommandId::Quit => MenuSection::App,
        _ => MenuSection::More,
    }
}

/// The submenu `id` goes into in the `⋯` menu: History, Keep history, New, Theme, Window
/// Size or Help.
pub fn menu_submenu(id: &CommandId) -> Option<&'static str> {
    match id {
        CommandId::History(_)
        | CommandId::DiffToggle(_)
        | CommandId::ClearHistory
        | CommandId::ClearPins => Some(crate::locale::t("history")),
        CommandId::KeepHistory(_) => Some(crate::locale::t("keep_history_menu")),
        CommandId::Generate(_) => Some(crate::locale::t("new_menu")),
        CommandId::Appearance(_) => Some(crate::locale::t("appearance")),
        CommandId::CardTheme(_) => Some(crate::locale::t("theme_menu")),
        CommandId::WindowSize(_) => Some(crate::locale::t("window_size_menu")),
        CommandId::Welcome
        | CommandId::SuggestFeature
        | CommandId::ReportProblem
        | CommandId::ExportDiagnostics
        | CommandId::CheckAi => Some(crate::locale::t("help_menu")),
        _ => None,
    }
}

/// Title of a [`CommandId::WindowSize`] choice, NL/EN.
pub fn window_size_title(size: crate::settings::WindowSize) -> &'static str {
    use crate::settings::WindowSize;
    match size {
        WindowSize::Compact => crate::locale::t("window_compact"),
        WindowSize::Normal => crate::locale::t("window_normal"),
        WindowSize::Wide => crate::locale::t("window_wide"),
    }
}

/// Short title of a retention choice inside the Keep history submenu: "15 min", "Tot wissen".
pub fn keep_history_short(minutes: u32) -> String {
    if minutes == 0 {
        crate::locale::t("keep_until_cleared").to_string()
    } else {
        format!("{minutes} min")
    }
}
