//! VoiceOver helpers shared by the drop target and the toast, and custom actions for controls.

use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSAccessibilityAnnouncementKey, NSAccessibilityAnnouncementRequestedNotification,
    NSAccessibilityPostNotificationWithUserInfo, NSAccessibilityPriorityKey,
    NSAccessibilityPriorityLevel, NSView,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString};

/// How urgently VoiceOver should speak an announcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// Queued after what VoiceOver is saying: confirmations such as the toast.
    Medium,
    /// Interrupts: the drop target's feedback while dragging.
    High,
}

impl Priority {
    fn level(self) -> NSAccessibilityPriorityLevel {
        match self {
            Priority::Medium => NSAccessibilityPriorityLevel::Medium,
            Priority::High => NSAccessibilityPriorityLevel::High,
        }
    }
}

/// Have VoiceOver say `text` right away, at high priority.
pub fn announce(element: &NSView, text: &str) {
    announce_at(element, text, Priority::High);
}

/// Have VoiceOver say `text` at `priority`.
pub fn announce_at(element: &NSView, text: &str, priority: Priority) {
    let text = NSString::from_str(text);
    let priority = NSNumber::numberWithInteger(priority.level().0);
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

/// Give `element` VoiceOver actions (the Actions rotor, VO-⌘-Space): each a name and what runs
/// when it is chosen, on the main thread. Replaces the actions it had; an empty list removes
/// them.
#[cfg(feature = "widgets")]
pub fn set_custom_actions(element: &NSView, actions: Vec<(String, Box<dyn Fn() + 'static>)>) {
    use objc2::AllocAnyThread;
    use objc2::runtime::Bool;
    use objc2_app_kit::{NSAccessibility, NSAccessibilityCustomAction};
    let made: Vec<objc2::rc::Retained<NSAccessibilityCustomAction>> = actions
        .into_iter()
        .map(|(name, run)| {
            let handler = block2::RcBlock::new(move || -> Bool {
                run();
                Bool::YES
            });
            NSAccessibilityCustomAction::initWithName_handler(
                NSAccessibilityCustomAction::alloc(),
                &NSString::from_str(&name),
                Some(&handler),
            )
        })
        .collect();
    element.setAccessibilityCustomActions(Some(&objc2_foundation::NSArray::from_retained_slice(
        &made,
    )));
}
