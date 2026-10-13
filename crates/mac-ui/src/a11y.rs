//! VoiceOver helpers shared by the drop target and the toast.

use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSAccessibilityAnnouncementKey, NSAccessibilityAnnouncementRequestedNotification,
    NSAccessibilityPostNotificationWithUserInfo, NSAccessibilityPriorityKey,
    NSAccessibilityPriorityLevel, NSView,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString};

/// Have VoiceOver say `text` right away, at high priority.
pub fn announce(element: &NSView, text: &str) {
    let text = NSString::from_str(text);
    let priority = NSNumber::numberWithInteger(NSAccessibilityPriorityLevel::High.0);
    let text: &AnyObject = &text;
    let priority: &AnyObject = &priority;
    // SAFETY (both): immutable AppKit constants, read for the life of the process.
    let info = NSDictionary::from_slices(
        &[unsafe { NSAccessibilityAnnouncementKey }, unsafe {
            NSAccessibilityPriorityKey
        }],
        &[text, priority],
    );
    // SAFETY: an announcement request takes a view and a dictionary of NSString keys with
    // an NSString announcement and an NSNumber priority.
    unsafe {
        NSAccessibilityPostNotificationWithUserInfo(
            element,
            NSAccessibilityAnnouncementRequestedNotification,
            Some(&info),
        );
    }
}
