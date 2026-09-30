//! App activation shared by `panel` and `dialog`.

use objc2::runtime::NSObjectProtocol;
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::NSApplication;

/// Bring the app to the front, even while another app is active. For menu bar (accessory) apps
/// that open a panel or alert from a hotkey or menu.
///
/// macOS 14+ has `activate` (cooperative activation) and deprecates
/// `activateIgnoringOtherApps:`; the old call is still made afterwards because it keeps working
/// in cases where cooperative activation is refused. Callers that must be visible regardless
/// also order their window front with `orderFrontRegardless`.
pub fn activate_app(mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    if app.respondsToSelector(sel!(activate)) {
        app.activate();
    }
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
}
