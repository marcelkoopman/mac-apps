//! Local notifications (UserNotifications, through `mac_ui::notify`): when to show one, and what
//! it says. The decision is the pure [`decide`]; the Mac side is `macos_notify.rs`.
//!
//! Privacy: a notification never holds clipboard text, an AI result, a file name or a line
//! number. The text is a fixed sentence, or (AI) the name of the action the user clicked. Nothing
//! about notifications or their content is logged. CopyCraft stays offline: these are local
//! notifications only (no push, no `aps-environment`, no network entitlement).

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::time::{Duration, Instant};

/// PDF text recognition shorter than this is not worth a notification: the user is still there.
pub const OCR_MIN: Duration = Duration::from_secs(3);
/// At most one notification per this long, whatever the kind.
pub const RATE_LIMIT: Duration = Duration::from_secs(5);

/// What a notification is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An AI action finished with a result.
    AiDone,
    /// Text recognition of a PDF took a while and is done.
    OcrDone,
    /// Error: the pin list is full.
    PinListFull,
    /// Error: Apple Intelligence is not available.
    AiUnavailable,
    /// A copy may hold a secret.
    SecretCopied,
}

impl Kind {
    pub const ALL: [Kind; 5] = [
        Self::AiDone,
        Self::OcrDone,
        Self::PinListFull,
        Self::AiUnavailable,
        Self::SecretCopied,
    ];

    /// The notification identifier: one per kind, so a new one replaces the old and the card
    /// can remove it.
    pub fn id(self) -> &'static str {
        match self {
            Self::AiDone => "copycraft.ai-done",
            Self::OcrDone => "copycraft.ocr-done",
            Self::PinListFull => "copycraft.error.pin-list-full",
            Self::AiUnavailable => "copycraft.error.ai-unavailable",
            Self::SecretCopied => "copycraft.secret-copied",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }

    /// The Settings switch this kind follows.
    pub fn switch(self) -> Switch {
        match self {
            Self::AiDone => Switch::Ai,
            Self::OcrDone => Switch::Ocr,
            Self::PinListFull | Self::AiUnavailable => Switch::Errors,
            Self::SecretCopied => Switch::Secret,
        }
    }

    fn is_error(self) -> bool {
        self.switch() == Switch::Errors
    }
}

/// The per-kind switches in Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    Ai,
    Ocr,
    Errors,
    Secret,
}

/// The state of the switches (all on by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switches {
    pub ai: bool,
    pub ocr: bool,
    pub errors: bool,
    pub secret: bool,
}

impl Switches {
    pub fn is_on(self, switch: Switch) -> bool {
        match switch {
            Switch::Ai => self.ai,
            Switch::Ocr => self.ocr,
            Switch::Errors => self.errors,
            Switch::Secret => self.secret,
        }
    }
}

/// Whether CopyCraft may show notifications, as last heard from macOS. Asked only the first
/// time one is needed. A refusal is not final: macOS is checked again (without asking) when
/// Settings › General opens and before a notification that only the refusal held back, and
/// a permission given since in System Settings turns it into [`Permission::Granted`]
/// ([`Permission::from_answer`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Permission {
    /// Not asked yet.
    #[default]
    Unknown,
    Granted,
    /// macOS said no: no notifications until a check finds them allowed again.
    Refused,
}

impl Permission {
    pub fn id(self) -> i64 {
        match self {
            Self::Unknown => 0,
            Self::Granted => 1,
            Self::Refused => 2,
        }
    }

    /// An unknown or missing value is [`Permission::Unknown`].
    pub fn from_id(id: i64) -> Self {
        match id {
            1 => Self::Granted,
            2 => Self::Refused,
            _ => Self::Unknown,
        }
    }

    /// What to store after macOS answered a check or its own question: allowed is
    /// [`Permission::Granted`] (also after a refusal), denied [`Permission::Refused`], and not
    /// asked yet (reset in System Settings, or an old refusal of only the explanation)
    /// [`Permission::Unknown`], so the next notification asks again.
    pub fn from_answer(answer: Answer) -> Self {
        match answer {
            Answer::Granted => Self::Granted,
            Answer::Denied => Self::Refused,
            Answer::NotDetermined => Self::Unknown,
        }
    }
}

/// What the system said about notifications, or what the user answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// The system has not asked the user yet.
    NotDetermined,
    Denied,
    Granted,
}

/// The situation an event happens in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    /// The card is not on screen.
    pub card_hidden: bool,
    /// CopyCraft is the active app.
    pub app_frontmost: bool,
    /// The run or job failed (AI).
    pub failed: bool,
    /// How long the job took (PDF recognition).
    pub took: Duration,
}

/// What [`decide`] says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Send,
    /// Worth sending, but the permission was never asked: explain and ask first.
    AskPermission,
    Skip(Skip),
}

/// Why nothing is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// The user refused notifications.
    Refused,
    /// The kind is switched off in Settings.
    SwitchedOff,
    /// A failed run says nothing.
    Failed,
    /// The user is looking at the card in the active app.
    Watching,
    /// Done within [`OCR_MIN`].
    Quick,
    /// This error was already notified this session.
    AlreadyTold,
    /// Another notification was shown less than [`RATE_LIMIT`] ago.
    RateLimited,
}

/// What this session has shown: the time of the last notification and the errors told.
#[derive(Debug, Clone, Default)]
pub struct Session {
    last: Option<Instant>,
    errors: Vec<Kind>,
}

impl Session {
    /// A notification of `kind` was posted at `now`.
    pub fn sent(&mut self, kind: Kind, now: Instant) {
        self.last = Some(now);
        if kind.is_error() && !self.errors.contains(&kind) {
            self.errors.push(kind);
        }
    }
}

/// A notification [`decide`] held back only because it was refused: one it would send if
/// macOS allowed it now. Worth checking the permission again (without asking) first.
pub fn worth_rechecking(
    kind: Kind,
    facts: Facts,
    switches: Switches,
    permission: Permission,
    session: &Session,
    now: Instant,
) -> bool {
    permission == Permission::Refused
        && decide(kind, facts, switches, Permission::Granted, session, now) == Verdict::Send
}

/// Whether to show a notification of `kind`. Checked in this order: refused, the kind's switch,
/// the rules of the kind, the rate limit, and last whether the permission still has to be asked.
///
/// - [`Kind::AiDone`]: only after a result (not a failure), and only when the card is hidden or
///   CopyCraft is not the active app.
/// - [`Kind::OcrDone`]: only when it took at least [`OCR_MIN`] and the card is hidden.
/// - errors: the same visibility rule as AI, and once per kind per session.
/// - [`Kind::SecretCopied`]: always (the copy came from another app, so the card is no help).
pub fn decide(
    kind: Kind,
    facts: Facts,
    switches: Switches,
    permission: Permission,
    session: &Session,
    now: Instant,
) -> Verdict {
    if permission == Permission::Refused {
        return Verdict::Skip(Skip::Refused);
    }
    if !switches.is_on(kind.switch()) {
        return Verdict::Skip(Skip::SwitchedOff);
    }
    let away = facts.card_hidden || !facts.app_frontmost;
    match kind {
        Kind::AiDone => {
            if facts.failed {
                return Verdict::Skip(Skip::Failed);
            }
            if !away {
                return Verdict::Skip(Skip::Watching);
            }
        }
        Kind::OcrDone => {
            if facts.failed {
                return Verdict::Skip(Skip::Failed);
            }
            if facts.took < OCR_MIN {
                return Verdict::Skip(Skip::Quick);
            }
            if !facts.card_hidden {
                return Verdict::Skip(Skip::Watching);
            }
        }
        Kind::PinListFull | Kind::AiUnavailable => {
            if session.errors.contains(&kind) {
                return Verdict::Skip(Skip::AlreadyTold);
            }
            if !away {
                return Verdict::Skip(Skip::Watching);
            }
        }
        Kind::SecretCopied => {}
    }
    if session
        .last
        .is_some_and(|last| now.saturating_duration_since(last) < RATE_LIMIT)
    {
        return Verdict::Skip(Skip::RateLimited);
    }
    if permission == Permission::Unknown {
        Verdict::AskPermission
    } else {
        Verdict::Send
    }
}

/// The notification's title: the app name.
pub const TITLE: &str = "CopyCraft";

/// The notification's text. Never clipboard or result content: `action` is the AI action's name
/// (a menu title), and the rest are fixed sentences.
pub fn body(kind: Kind, action: Option<&str>) -> String {
    match kind {
        Kind::AiDone => crate::locale::tf("notify_ai_done", &[&action.unwrap_or("AI")]),
        Kind::OcrDone => crate::locale::t("notify_ocr_done").to_string(),
        Kind::PinListFull => crate::locale::t("notify_pin_full").to_string(),
        Kind::AiUnavailable => crate::locale::t("notify_ai_unavailable").to_string(),
        Kind::SecretCopied => crate::locale::t("notify_secret").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::{Lang, t_in, tf_in};

    const ON: Switches = Switches {
        ai: true,
        ocr: true,
        errors: true,
        secret: true,
    };

    fn away() -> Facts {
        Facts {
            card_hidden: true,
            app_frontmost: false,
            failed: false,
            took: Duration::from_secs(10),
        }
    }

    fn check(kind: Kind, facts: Facts) -> Verdict {
        decide(
            kind,
            facts,
            ON,
            Permission::Granted,
            &Session::default(),
            Instant::now(),
        )
    }

    #[test]
    fn ai_done_needs_a_hidden_card_or_another_active_app() {
        assert_eq!(check(Kind::AiDone, away()), Verdict::Send);
        let hidden_but_active = Facts {
            app_frontmost: true,
            ..away()
        };
        assert_eq!(check(Kind::AiDone, hidden_but_active), Verdict::Send);
        let shown_in_other_app = Facts {
            card_hidden: false,
            app_frontmost: false,
            ..away()
        };
        assert_eq!(check(Kind::AiDone, shown_in_other_app), Verdict::Send);
        let watching = Facts {
            card_hidden: false,
            app_frontmost: true,
            ..away()
        };
        assert_eq!(check(Kind::AiDone, watching), Verdict::Skip(Skip::Watching));
    }

    #[test]
    fn a_failed_run_gives_no_notification() {
        let failed = Facts {
            failed: true,
            ..away()
        };
        assert_eq!(check(Kind::AiDone, failed), Verdict::Skip(Skip::Failed));
    }

    #[test]
    fn ocr_needs_three_seconds_and_a_hidden_card() {
        let quick = Facts {
            took: Duration::from_millis(2999),
            ..away()
        };
        assert_eq!(check(Kind::OcrDone, quick), Verdict::Skip(Skip::Quick));
        let slow = Facts {
            took: OCR_MIN,
            ..away()
        };
        assert_eq!(check(Kind::OcrDone, slow), Verdict::Send);
        // The card is open (even in the background): the user sees the result arrive.
        let shown = Facts {
            card_hidden: false,
            app_frontmost: false,
            ..slow
        };
        assert_eq!(check(Kind::OcrDone, shown), Verdict::Skip(Skip::Watching));
    }

    #[test]
    fn an_error_is_told_once_per_kind_per_session() {
        let now = Instant::now();
        let mut session = Session::default();
        let verdict = |session: &Session, kind, now| {
            decide(kind, away(), ON, Permission::Granted, session, now)
        };
        assert_eq!(verdict(&session, Kind::AiUnavailable, now), Verdict::Send);
        session.sent(Kind::AiUnavailable, now);
        let later = now + RATE_LIMIT * 10;
        assert_eq!(
            verdict(&session, Kind::AiUnavailable, later),
            Verdict::Skip(Skip::AlreadyTold)
        );
        // Another kind of error still gets its one.
        assert_eq!(verdict(&session, Kind::PinListFull, later), Verdict::Send);
        // Errors you are looking at are not notified.
        let watching = Facts {
            card_hidden: false,
            app_frontmost: true,
            ..away()
        };
        assert_eq!(
            decide(
                Kind::PinListFull,
                watching,
                ON,
                Permission::Granted,
                &Session::default(),
                now
            ),
            Verdict::Skip(Skip::Watching)
        );
    }

    #[test]
    fn a_secret_copy_is_notified_wherever_the_user_is() {
        let watching = Facts {
            card_hidden: false,
            app_frontmost: true,
            ..away()
        };
        assert_eq!(check(Kind::SecretCopied, watching), Verdict::Send);
        assert_eq!(check(Kind::SecretCopied, away()), Verdict::Send);
    }

    #[test]
    fn every_kind_follows_its_switch() {
        for kind in Kind::ALL {
            let mut switches = ON;
            match kind.switch() {
                Switch::Ai => switches.ai = false,
                Switch::Ocr => switches.ocr = false,
                Switch::Errors => switches.errors = false,
                Switch::Secret => switches.secret = false,
            }
            assert_eq!(
                decide(
                    kind,
                    away(),
                    switches,
                    Permission::Granted,
                    &Session::default(),
                    Instant::now()
                ),
                Verdict::Skip(Skip::SwitchedOff),
                "{kind:?}"
            );
        }
        // One switch covers both errors; the others are separate.
        assert_eq!(Kind::PinListFull.switch(), Kind::AiUnavailable.switch());
        assert_ne!(Kind::AiDone.switch(), Kind::OcrDone.switch());
    }

    #[test]
    fn at_most_one_notification_per_rate_limit() {
        let now = Instant::now();
        let mut session = Session::default();
        session.sent(Kind::SecretCopied, now);
        let verdict = |at| decide(Kind::AiDone, away(), ON, Permission::Granted, &session, at);
        assert_eq!(verdict(now), Verdict::Skip(Skip::RateLimited));
        assert_eq!(
            verdict(now + RATE_LIMIT - Duration::from_millis(1)),
            Verdict::Skip(Skip::RateLimited)
        );
        assert_eq!(verdict(now + RATE_LIMIT), Verdict::Send);
    }

    #[test]
    fn a_refusal_is_checked_again_and_undone_by_a_permission() {
        let now = Instant::now();
        let session = Session::default();
        let recheck = |kind, facts, switches, permission| {
            worth_rechecking(kind, facts, switches, permission, &session, now)
        };
        // Refused, and it would go out if allowed: check macOS again.
        assert!(recheck(Kind::AiDone, away(), ON, Permission::Refused));
        assert!(recheck(Kind::SecretCopied, away(), ON, Permission::Refused));
        // Not refused: nothing to check again.
        assert!(!recheck(Kind::AiDone, away(), ON, Permission::Granted));
        assert!(!recheck(Kind::AiDone, away(), ON, Permission::Unknown));
        // Held back for another reason too: no check.
        let failed = Facts {
            failed: true,
            ..away()
        };
        assert!(!recheck(Kind::AiDone, failed, ON, Permission::Refused));
        let off = Switches { ai: false, ..ON };
        assert!(!recheck(Kind::AiDone, away(), off, Permission::Refused));
        // What macOS answers is what is stored: a permission given in System Settings undoes
        // the refusal, a reset asks again.
        assert_eq!(
            Permission::from_answer(Answer::Granted),
            Permission::Granted
        );
        assert_eq!(Permission::from_answer(Answer::Denied), Permission::Refused);
        assert_eq!(
            Permission::from_answer(Answer::NotDetermined),
            Permission::Unknown
        );
    }

    #[test]
    fn permission_is_asked_once_and_a_refusal_holds_until_checked_again() {
        let ask = |permission| {
            decide(
                Kind::AiDone,
                away(),
                ON,
                permission,
                &Session::default(),
                Instant::now(),
            )
        };
        assert_eq!(ask(Permission::Unknown), Verdict::AskPermission);
        assert_eq!(ask(Permission::Granted), Verdict::Send);
        assert_eq!(ask(Permission::Refused), Verdict::Skip(Skip::Refused));
        // Nothing worth sending: nothing to ask for either.
        let failed = Facts {
            failed: true,
            ..away()
        };
        assert_eq!(
            decide(
                Kind::AiDone,
                failed,
                ON,
                Permission::Unknown,
                &Session::default(),
                Instant::now()
            ),
            Verdict::Skip(Skip::Failed)
        );
        // Refused wins over everything (until a check undoes it), and the stored value
        // round-trips.
        for permission in [
            Permission::Unknown,
            Permission::Granted,
            Permission::Refused,
        ] {
            assert_eq!(Permission::from_id(permission.id()), permission);
        }
        assert_eq!(Permission::from_id(9), Permission::Unknown);
    }

    #[test]
    fn identifiers_are_unique_and_map_back() {
        for kind in Kind::ALL {
            assert_eq!(Kind::from_id(kind.id()), Some(kind));
        }
        assert_eq!(Kind::from_id("other"), None);
    }

    #[test]
    fn texts_hold_only_the_action_name_or_a_fixed_sentence() {
        for lang in [Lang::Nl, Lang::En] {
            assert_eq!(
                tf_in(lang, "notify_ai_done", &[&t_in(lang, "ai_summarize")]),
                if lang == Lang::Nl {
                    "AI-actie klaar: Vat samen"
                } else {
                    "AI action done: Summarize"
                }
            );
            for key in [
                "notify_ocr_done",
                "notify_pin_full",
                "notify_ai_unavailable",
                "notify_secret",
                "notify_ask_title",
                "notify_ask_message",
                "notify_ask_ok",
                "notify_ask_no",
            ] {
                assert_ne!(t_in(lang, key), "?", "{key}");
            }
        }
        // The secret notice says only that, in both languages.
        assert_eq!(
            t_in(Lang::En, "notify_secret"),
            "Possibly sensitive text copied"
        );
        assert_eq!(
            t_in(Lang::Nl, "notify_secret"),
            "Mogelijk gevoelige tekst gekopieerd"
        );
        // The body is built from the action name alone.
        assert_eq!(
            body(Kind::SecretCopied, Some("secret text")),
            body(Kind::SecretCopied, None)
        );
        assert!(!body(Kind::OcrDone, Some("secret text")).contains("secret text"));
    }
}
