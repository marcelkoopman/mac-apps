//! App activation shared by `panel` and `dialog`.

use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;

/// Bring the app to the front, even while another app is active. For menu bar (accessory) apps
/// that open a panel or alert from a hotkey or menu.
pub fn activate_app(mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
}
