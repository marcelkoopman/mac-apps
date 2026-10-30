//! Open at Login through `SMAppService` (macOS 13+), without a helper, typed through
//! objc2-service-management.
//!
//! Swift calls the class property `mainApp` (`NS_SWIFT_NAME`); the Objective-C getter (and the
//! binding) is `+mainAppService`. The class and its selectors are checked at run time first:
//! if one is missing, Open at Login is unavailable and the Settings window still opens.
//! No network: the
//! deep link only opens System Settings › Login Items when approval is required.

// Loads ServiceManagement so `SMAppService` is registered (the crate's own link covers only its
// statics, which are not used). `#[link]` here has no `+weak` modifier (rustc rejects it); a missing
// class or `+mainAppService` is handled at runtime instead of aborting.
// SAFETY: an empty block: it only links the framework and declares no symbols.
#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

use mac_ui::objc2::encode::{EncodeArguments, EncodeReturn};
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
use mac_ui::objc2::sel;
use mac_ui::objc2_foundation::NSError;
use objc2_service_management::{SMAppService, SMAppServiceStatus};

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

fn main_app() -> Option<Retained<SMAppService>> {
    let class = AnyClass::get(c"SMAppService")?;
    // `+mainAppService` returns an autoreleased `SMAppService *` (`@`).
    if !class_selector::<(), *mut AnyObject>(class, sel!(mainAppService)) {
        return None;
    }
    // SAFETY: the class and selector were verified above; the binding retains the result.
    Some(unsafe { SMAppService::mainAppService() })
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
    match unsafe { service.status() } {
        SMAppServiceStatus::NotRegistered => LoginStatus::NotRegistered,
        SMAppServiceStatus::Enabled => LoginStatus::Enabled,
        SMAppServiceStatus::RequiresApproval => LoginStatus::RequiresApproval,
        SMAppServiceStatus::NotFound => LoginStatus::NotFound,
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
    // SAFETY: `-registerAndReturnError:` / `-unregisterAndReturnError:` were verified above; the
    // binding turns NO plus the out-error into `Err`.
    let changed = unsafe {
        if on {
            service.registerAndReturnError()
        } else {
            service.unregisterAndReturnError()
        }
    };
    if let Err(error) = changed {
        // NO without an NSError: objc2 hands over its placeholder error, shown as before.
        return Err(if error.domain().to_string() == "__objc2.missingError" {
            crate::locale::t("login_change_failed").into()
        } else {
            error.localizedDescription().to_string()
        });
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
    unsafe { SMAppService::openSystemSettingsLoginItems() };
}
