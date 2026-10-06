//! Open at Login through `SMAppService` (macOS 13+), without a helper or a new crate.
//!
//! Swift calls the class property `mainApp` (`NS_SWIFT_NAME`). The Objective-C getter is
//! `+mainAppService`. Sending `mainApp` makes objc2 panic in debug builds ("method not found")
//! while Settings is opening. If the class or selector is missing, Open at Login is unavailable
//! and the Settings window still opens.
//!
//! Called at runtime so older SDKs and the Linux clippy target still build. No network: the
//! deep link only opens System Settings › Login Items when approval is required.

// Loads ServiceManagement so `SMAppService` is registered. No direct symbol refs: those are
// absent from older SDKs. `#[link]` here has no `+weak` modifier (rustc rejects it); a missing
// class or `+mainAppService` is handled at runtime instead of aborting.
#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

use mac_ui::objc2::encode::{EncodeArguments, EncodeReturn};
use mac_ui::objc2::msg_send;
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
use mac_ui::objc2::sel;
use mac_ui::objc2_foundation::NSError;

/// Status of the main app as a login item ([`SMAppServiceStatus`](https://developer.apple.com/documentation/servicemanagement/smappservice/status-swift.property)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginStatus {
    NotRegistered = 0,
    Enabled = 1,
    RequiresApproval = 2,
    NotFound = 3,
    /// `SMAppService` or `+mainAppService` is missing (before macOS 13), or the call failed.
    Unavailable = -1,
}

impl LoginStatus {
    pub fn is_on(self) -> bool {
        matches!(self, Self::Enabled | Self::RequiresApproval)
    }
}

/// Class methods live on the metaclass. `verify_sel` checks the selector and the
/// encoding objc2 will assert, and returns an error instead of panicking.
fn class_selector<A, R>(class: &AnyClass, selector: Sel) -> bool
where
    A: EncodeArguments,
    R: EncodeReturn,
{
    class.metaclass().verify_sel::<A, R>(selector).is_ok()
}

fn instance_selector<A, R>(object: &AnyObject, selector: Sel) -> bool
where
    A: EncodeArguments,
    R: EncodeReturn,
{
    object.class().verify_sel::<A, R>(selector).is_ok()
}

fn main_app() -> Option<Retained<AnyObject>> {
    let class = AnyClass::get(c"SMAppService")?;
    // `+mainAppService` returns an autoreleased `SMAppService *` (`@`).
    if !class_selector::<(), *mut AnyObject>(class, sel!(mainAppService)) {
        return None;
    }
    // SAFETY: selector and `@` return were verified above. Not a +1 method family,
    // so `Option<Retained<_>>` retains the autoreleased object and accepts NULL.
    unsafe { msg_send![class, mainAppService] }
}

/// Current Open at Login status.
pub fn status() -> LoginStatus {
    let Some(service) = main_app() else {
        return LoginStatus::Unavailable;
    };
    // `-status` returns `SMAppServiceStatus` (`NSInteger`, `q` on 64-bit).
    if !instance_selector::<(), isize>(&service, sel!(status)) {
        return LoginStatus::Unavailable;
    }
    // SAFETY: `-status` was verified above and takes no arguments.
    let raw: isize = unsafe { msg_send![&service, status] };
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
        return Err(crate::locale::t("login_unavailable").into());
    };
    let selector = if on {
        sel!(registerAndReturnError:)
    } else {
        sel!(unregisterAndReturnError:)
    };
    // `BOOL` + one `NSError * _Nullable *` (`^@`).
    if !instance_selector::<(&mut *mut NSError,), Bool>(&service, selector) {
        return Err(crate::locale::t("login_unavailable").into());
    }
    let mut error: *mut NSError = std::ptr::null_mut();
    let ok: Bool = if on {
        // SAFETY: `-[SMAppService registerAndReturnError:]` was verified above.
        unsafe { msg_send![&service, registerAndReturnError: &mut error] }
    } else {
        // SAFETY: `-[SMAppService unregisterAndReturnError:]` was verified above.
        unsafe { msg_send![&service, unregisterAndReturnError: &mut error] }
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
    if !class_selector::<(), ()>(class, sel!(openSystemSettingsLoginItems)) {
        return;
    }
    // SAFETY: `+[SMAppService openSystemSettingsLoginItems]` was verified above.
    let _: () = unsafe { msg_send![class, openSystemSettingsLoginItems] };
}
