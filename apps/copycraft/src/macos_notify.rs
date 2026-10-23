//! The Mac side of [`crate::notifications`]: posting and removing notifications through
//! `mac_ui::notify`, the permission question and the click. Local notifications only; nothing
//! here is logged, and nothing but the fixed texts of `notifications::body` is ever posted.

use mac_ui::objc2::MainThreadMarker;
use mac_ui::objc2_app_kit::NSApplication;

use crate::launcher::{self, NotificationEvent, UserEvent};
use crate::notifications::{Answer, Kind, TITLE};

/// Hear clicks on notifications. Once, at startup; asks the user nothing.
pub fn init() {
    mac_ui::notify::set_click_handler(clicked);
}

fn clicked(id: &str) {
    if let Some(kind) = Kind::from_id(id) {
        launcher::emit(UserEvent::Notification(NotificationEvent::Clicked(kind)));
    }
}

/// CopyCraft is the active app.
pub fn app_is_frontmost() -> bool {
    MainThreadMarker::new().is_some_and(|mtm| NSApplication::sharedApplication(mtm).isActive())
}

/// Show the notification of `kind` (replacing an older one of the same kind). `false` without
/// an app bundle (`cargo run`).
pub fn post(kind: Kind, action: Option<&str>) -> bool {
    mac_ui::notify::post(kind.id(), TITLE, &crate::notifications::body(kind, action))
}

/// Take the notification of `kind` off the screen and out of Notification Center.
pub fn remove(kind: Kind) {
    mac_ui::notify::remove(kind.id());
}

/// Ask the system what the user decided already, without asking the user. The answer comes back
/// as [`NotificationEvent::Permission`].
pub fn probe_permission() {
    mac_ui::notify::permission(|permission| {
        let answer = match permission {
            mac_ui::notify::Permission::NotDetermined => Answer::NotDetermined,
            mac_ui::notify::Permission::Denied => Answer::Denied,
            mac_ui::notify::Permission::Granted => Answer::Granted,
        };
        launcher::emit(UserEvent::Notification(NotificationEvent::Permission(
            answer,
        )));
    });
}

/// Explain, in a short dialog, why CopyCraft wants to send notifications, before the system's
/// question. One button, *Continue*: the choice is made in the system's question that follows,
/// not here.
pub fn explain(mtm: MainThreadMarker) {
    mac_ui::panel::activate_app(mtm);
    mac_ui::dialog::buttons(
        mtm,
        crate::locale::t("notify_ask_title"),
        crate::locale::t("notify_ask_message"),
        &[crate::locale::t("notify_ask_ok")],
    );
}

/// Show the system's permission question. The answer comes back as
/// [`NotificationEvent::Permission`].
pub fn request_permission() {
    mac_ui::notify::request_permission(|granted| {
        let answer = if granted {
            Answer::Granted
        } else {
            Answer::Denied
        };
        launcher::emit(UserEvent::Notification(NotificationEvent::Permission(
            answer,
        )));
    });
}
