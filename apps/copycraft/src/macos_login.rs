//! Open at Login through `SMAppService` (macOS 13+), without a helper or a new crate.
//!
//! Called at runtime so older SDKs and the Linux clippy target still build. No network: the
//! deep link only opens System Settings › Login Items when approval is required.

#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

use mac_ui::objc2::msg_send;
use mac_ui::objc2::runtime::{AnyClass, AnyObject, Bool};
use mac_ui::objc2_foundation::NSError;

/// Status of the main app as a login item ([`SMAppServiceStatus`](https://developer.apple.com/documentation/servicemanagement/smappservice/status-swift.property)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginStatus {
    NotRegistered = 0,
    Enabled = 1,
    RequiresApproval = 2,
    NotFound = 3,
    /// `SMAppService` is missing (before macOS 13) or the call failed.
    Unavailable = -1,
}

impl LoginStatus {
    pub fn is_on(self) -> bool {
        matches!(self, Self::Enabled | Self::RequiresApproval)
    }
}

fn main_app() -> Option<*const AnyObject> {
    let class = AnyClass::get(c"SMAppService")?;
    // SAFETY: SMAppService.mainApp is a class method returning an retained instance.
    let service: *const AnyObject = unsafe { msg_send![class, mainApp] };
    if service.is_null() {
        None
    } else {
        Some(service)
    }
}

/// Current Open at Login status.
pub fn status() -> LoginStatus {
    let Some(service) = main_app() else {
        return LoginStatus::Unavailable;
    };
    // SAFETY: -[SMAppService status] returns SMAppServiceStatus (NSInteger).
    let raw: isize = unsafe { msg_send![service, status] };
    match raw {
        0 => LoginStatus::NotRegistered,
        1 => LoginStatus::Enabled,
        2 => LoginStatus::RequiresApproval,
        3 => LoginStatus::NotFound,
        _ => LoginStatus::Unavailable,
    }
}

/// Turn Open at Login on or off. When the status becomes [`LoginStatus::RequiresApproval`],
/// opens System Settings › Login Items so the user can allow it.
pub fn set_enabled(on: bool) -> Result<LoginStatus, String> {
    let Some(service) = main_app() else {
        return Err("Open at Login needs macOS 13 or later".into());
    };
    let mut error: *mut NSError = std::ptr::null_mut();
    let ok: Bool = if on {
        // SAFETY: -[SMAppService registerAndReturnError:]
        unsafe { msg_send![service, registerAndReturnError: &mut error] }
    } else {
        // SAFETY: -[SMAppService unregisterAndReturnError:]
        unsafe { msg_send![service, unregisterAndReturnError: &mut error] }
    };
    if !ok.as_bool() {
        let message = if error.is_null() {
            "Could not change Open at Login".into()
        } else {
            // SAFETY: error is a non-null NSError* from the call above.
            let err: &NSError = unsafe { &*error };
            err.localizedDescription().to_string()
        };
        return Err(message);
    }
    let now = status();
    if on && now == LoginStatus::RequiresApproval {
        open_login_items_settings();
    }
    Ok(now)
}

/// Opens System Settings › Login Items (`SMAppService.openSystemSettingsLoginItems`).
pub fn open_login_items_settings() {
    let Some(class) = AnyClass::get(c"SMAppService") else {
        return;
    };
    // SAFETY: +[SMAppService openSystemSettingsLoginItems] takes no arguments.
    let _: () = unsafe { msg_send![class, openSystemSettingsLoginItems] };
}
