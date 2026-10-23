//! Local notifications ([`crate::notifications`]): the App side. The decision is
//! [`crate::notifications::decide`]; this asks the permission the first time one is needed,
//! checks it again (without asking) before a notification a refusal held back and when
//! Settings › General opens, posts it, takes it away again when the card opens, and opens the
//! card on a click. Nothing about a notification or its content is logged.

use super::*;
#[cfg_attr(not(target_os = "macos"), allow(unused_imports))]
use crate::notifications::{self, Answer, Facts, Kind, Permission, Verdict};

/// A notification waiting for the permission to be settled.
pub(crate) struct NoticeWait {
    kind: Kind,
    /// The AI action's name for [`Kind::AiDone`].
    action: Option<String>,
    /// The system's own question was asked (after the explanation).
    asked: bool,
}

/// What the App keeps about notifications: this session's record and what is on screen.
#[derive(Default)]
pub(crate) struct Notices {
    session: notifications::Session,
    /// Posted by this session and not yet taken away.
    posted: Vec<Kind>,
    /// The card was open when the last one was posted (it only comes off when the card opens
    /// again, not at once).
    posted_card_open: bool,
    waiting: Option<NoticeWait>,
    /// The pasteboard change a "possibly sensitive" notification was already shown for.
    secret_told: Option<isize>,
    /// A long copy whose labels are still being checked (its pasteboard change).
    secret_pending: Option<isize>,
    /// When the text recognition of the PDF on the card started.
    pub(crate) pdf_scan_started: Option<Instant>,
}

impl Notices {
    /// Whether a copy (pasteboard change `change`) is a credential not told about yet. A long
    /// copy whose labels are still being checked is remembered and looked at again later.
    fn secret_to_tell(&mut self, text: &str, change: isize) -> bool {
        match crate::sensitivity::labeling(text) {
            crate::sensitivity::Labeling::Known(found) => {
                self.secret_pending = None;
                let tell = found
                    .labels
                    .contains(&crate::sensitivity::Label::Credential)
                    && self.secret_told != Some(change);
                if tell {
                    self.secret_told = Some(change);
                }
                tell
            }
            crate::sensitivity::Labeling::Checking => {
                self.secret_pending = Some(change);
                false
            }
        }
    }
}

impl App {
    /// `kind` happened. `card_hidden` is read here; `failed` and `took` come from the caller.
    pub(crate) fn notify(
        &mut self,
        kind: Kind,
        failed: bool,
        took: Duration,
        action: Option<&str>,
    ) {
        #[cfg(target_os = "macos")]
        {
            let facts = Facts {
                card_hidden: !launcher::is_open(),
                app_frontmost: crate::macos_notify::app_is_frontmost(),
                failed,
                took,
            };
            let switches = crate::settings::notify_switches();
            let permission = crate::settings::notify_permission();
            let now = Instant::now();
            let session = &self.notices.session;
            // Refused may have been undone in System Settings: ask macOS (it never prompts)
            // and post it once the answer says allowed.
            if notifications::worth_rechecking(kind, facts, switches, permission, session, now) {
                self.notices.waiting = Some(NoticeWait {
                    kind,
                    action: action.map(str::to_string),
                    asked: false,
                });
                crate::macos_notify::probe_permission();
                return;
            }
            let verdict = notifications::decide(kind, facts, switches, permission, session, now);
            match verdict {
                Verdict::Send => self.post_notice(kind, action),
                Verdict::AskPermission => {
                    self.notices.waiting = Some(NoticeWait {
                        kind,
                        action: action.map(str::to_string),
                        asked: false,
                    });
                    crate::macos_notify::probe_permission();
                }
                Verdict::Skip(_) => {}
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (kind, failed, took, action);
    }

    #[cfg(target_os = "macos")]
    fn post_notice(&mut self, kind: Kind, action: Option<&str>) {
        if crate::macos_notify::post(kind, action) {
            self.notices.session.sent(kind, Instant::now());
            if !self.notices.posted.contains(&kind) {
                self.notices.posted.push(kind);
            }
            self.notices.posted_card_open = launcher::is_open();
        }
    }

    /// The permission question has an answer: from a check (before a notification, or when
    /// Settings › General opened), or from the system's question.
    pub(crate) fn notification_event(&mut self, event: launcher::NotificationEvent) {
        match event {
            launcher::NotificationEvent::Clicked(_) => self.notice_clicked(),
            launcher::NotificationEvent::Permission(answer) => self.notice_permission(answer),
        }
    }

    /// A click on a notification opens the card; with an AI run it comes back with its preview.
    fn notice_clicked(&mut self) {
        if !launcher::is_open() {
            self.summon_popup();
        }
    }

    fn notice_permission(&mut self, answer: Answer) {
        // What macOS says is what is stored, also a permission given since a refusal; an open
        // Settings window shows it (and dims the switches while refused).
        crate::settings::set_notify_permission(Permission::from_answer(answer));
        #[cfg(target_os = "macos")]
        crate::macos_settings::refresh_notifications();
        let Some(wait) = self.notices.waiting.take() else {
            return;
        };
        match answer {
            Answer::Granted => {
                #[cfg(target_os = "macos")]
                self.post_notice(wait.kind, wait.action.as_deref());
            }
            Answer::Denied => {}
            Answer::NotDetermined if !wait.asked => {
                #[cfg(target_os = "macos")]
                {
                    let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                        return;
                    };
                    // One way on: the explanation leads to the system's question, where the
                    // choice is made.
                    crate::macos_notify::explain(mtm);
                    self.notices.waiting = Some(NoticeWait {
                        asked: true,
                        ..wait
                    });
                    crate::macos_notify::request_permission();
                }
            }
            // Still undecided after the question: ask nothing more for this one.
            Answer::NotDetermined => {}
        }
    }

    /// The card opened: its notifications have done their job.
    pub(crate) fn clear_notices_when_card_opens(&mut self) {
        if !launcher::is_open() {
            self.notices.posted_card_open = false;
            return;
        }
        if self.notices.posted_card_open || self.notices.posted.is_empty() {
            return;
        }
        for kind in std::mem::take(&mut self.notices.posted) {
            #[cfg(target_os = "macos")]
            crate::macos_notify::remove(kind);
            #[cfg(not(target_os = "macos"))]
            let _ = kind;
        }
    }

    /// A copy from another app that may hold a secret: a notification, without the content.
    /// It does not follow the privacy filter (that only masks what is shown): whether it is
    /// posted is up to its own switch and the other notification settings in
    /// [`notifications::decide`]. A copy another app marked concealed is never read, so it does
    /// not get here.
    pub(crate) fn note_secret_copy(&mut self, text: &str, change: isize) {
        if self.notices.secret_to_tell(text, change) {
            self.notify(Kind::SecretCopied, false, Duration::ZERO, None);
        }
    }

    /// The checker has labels: a long copy that was waiting for them is looked at again.
    pub(crate) fn secret_labels_checked(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some(change) = self.notices.secret_pending
            && crate::macos_pasteboard::change_count() == change
        {
            let view = ClipboardView::from_os();
            if let Some(text) = view.text() {
                self.note_secret_copy(text, change);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_session_has_posted_nothing() {
        let notices = Notices::default();
        assert!(notices.posted.is_empty());
        assert!(notices.waiting.is_none());
    }

    #[test]
    fn a_secret_copy_is_told_whatever_the_privacy_filter_says() {
        let token = format!("ghp_{}", "1234567890abcdefghijklmnopqrstuvwxyzAB");
        for (filter, change) in [(false, 1), (true, 2)] {
            crate::settings::set_privacy_filter(filter);
            let mut notices = Notices::default();
            assert!(notices.secret_to_tell(&token, change), "filter {filter}");
            // Once per pasteboard change.
            assert!(!notices.secret_to_tell(&token, change), "filter {filter}");
            assert!(
                !notices.secret_to_tell("plain shopping list", change + 10),
                "filter {filter}"
            );
        }
        crate::settings::set_privacy_filter(false);
    }

    /// A polled copy of a token, the way [`App::record_current`] and [`App::note_secret_copy`]
    /// take it: with the filter off and on, it stays out of history and the notification goes
    /// out. (An `App` needs the menu bar item and the hotkey manager, so this runs their parts.)
    #[test]
    fn a_copied_token_stays_out_of_history_and_is_told_with_the_filter_off_and_on() {
        let token = format!("ghp_{}", "1234567890abcdefghijklmnopqrstuvwxyzAB");
        for (filter, change) in [(false, 1), (true, 2)] {
            crate::settings::set_privacy_filter(filter);
            assert_eq!(crate::settings::privacy_filter(), filter);
            let mut history = crate::clipboard::ClipboardHistory::default();
            assert_eq!(
                admit_text(&mut history, &token),
                TextAdmit::Refused,
                "filter {filter}"
            );
            assert!(history.is_empty(), "filter {filter}");
            assert_eq!(
                admit_text(&mut history, "plain shopping list"),
                TextAdmit::Kept
            );
            assert_eq!(history.len(), 1, "filter {filter}");
            let mut notices = Notices::default();
            assert!(notices.secret_to_tell(&token, change), "filter {filter}");
            let facts = Facts {
                card_hidden: true,
                app_frontmost: false,
                failed: false,
                took: Duration::ZERO,
            };
            assert_eq!(
                notifications::decide(
                    Kind::SecretCopied,
                    facts,
                    crate::settings::notify_switches(),
                    Permission::Granted,
                    &notices.session,
                    Instant::now(),
                ),
                Verdict::Send,
                "filter {filter}"
            );
        }
        crate::settings::set_privacy_filter(false);
    }
}
