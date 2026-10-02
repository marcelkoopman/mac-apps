//! Drop target: a view that takes one dropped file or dropped text.
//!
//! [`target`] builds an `NSView` registered for file URLs and/or plain text. Make it the window's
//! content view (or another container) and build the content inside it. AppKit offers a drag to
//! the deepest registered view under the pointer, so plain views, buttons, glass and read-only
//! text and image views inside it leave the drag to the target. Editable text (a field being
//! edited, an editable text view) registers itself and keeps taking its own text drops; give the
//! window [`field_editor_without_drops`] when the whole window should take them instead.
//!
//! While the drag moves it is judged by [`judge`] on what the drag pasteboard declares, without
//! reading any text or file: one file whose type conforms to one of [`Accept::file_types`], or
//! text when [`Accept::text`] is set. Anything else, including several files, folders, a file of
//! another type or file promises, gets `NSDragOperation::None`, so the pointer shows that it
//! cannot drop there. A file whose name maps to no known type is let through: only its contents
//! can tell, so the caller judges it after the drop. The drop itself is read once, on the main
//! thread, and handed to the callback as [`Dropped`].
//!
//! With a [`Highlight`], a drag that can be dropped outlines the target in the accent colour
//! (system colours only, so it follows the appearance and the user's accent) and VoiceOver
//! announces it.
//!
//! [`onto_tray`] takes drops on a menu bar icon instead: a click-through drop view over the
//! status item's button, which lights up the button like an open menu while a drag that can be
//! dropped is over it. Only public AppKit (`NSStatusItem.button`) is used; the tray icon's own
//! views and click handling are left alone.

use std::path::PathBuf;

/// What a drop delivers to the [`target`] callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dropped {
    /// One file, by path. Reading it is up to the caller; in the App Sandbox the drop has given
    /// the app access to it (no security scope to start or stop).
    File(PathBuf),
    /// The dropped text. The caller owns it, and should move it into a zeroizing wrapper right
    /// away if it can be sensitive.
    Text(String),
}

/// What a [`target`] takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accept {
    /// Uniform type identifiers such as `"public.text"`. A dropped file is taken when its type
    /// conforms to one of them. Empty: no files.
    pub file_types: &'static [&'static str],
    /// Take dropped plain text (`NSPasteboardTypeString`) when the drag has no files.
    pub text: bool,
}

/// How a [`target`] shows that the drag over it can be dropped: an outline in the accent colour
/// over a faint selection tint, on top of the target's content, and a VoiceOver announcement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Highlight {
    /// Distance from the target's edges to the outline.
    pub inset: f64,
    /// Corner radius of the outline. Inside a rounded target, use
    /// [`concentric_radius`](crate::corners::concentric_radius)`(outer, inset)`.
    pub corner_radius: f64,
    /// What VoiceOver says when a drag that can be dropped enters, such as `"Drop to open"`.
    pub announcement: &'static str,
}

/// One file in a drag, as far as its URL tells before the drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// A folder or package (a directory URL).
    Directory,
    /// Its type conforms to one of [`Accept::file_types`].
    Accepted,
    /// A known type that conforms to none of them.
    Other,
    /// No known type: no extension, or one the system has no type for.
    Unknown,
}

/// What a drag will deliver if it is dropped, see [`judge`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    File,
    Text,
}

/// Whether a drag holding `files` (and text, when `has_text`) can be dropped under `accept`.
///
/// A drag with files is judged by its files only: exactly one, not a directory, of an accepted
/// or unknown type. Its text never stands in, because for a file drag that is just the file's
/// name or path. A drag without files offers text when it has some and `accept.text` is set.
pub fn judge(files: &[FileKind], has_text: bool, accept: Accept) -> Option<Offer> {
    match files {
        [] => (has_text && accept.text).then_some(Offer::Text),
        [FileKind::Accepted | FileKind::Unknown] if !accept.file_types.is_empty() => {
            Some(Offer::File)
        }
        _ => None,
    }
}

#[cfg(target_os = "macos")]
pub use appkit::{field_editor_without_drops, onto_tray, target};

#[cfg(target_os = "macos")]
mod appkit {
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;

    use objc2::rc::{Retained, autoreleasepool};
    use objc2::runtime::{AnyClass, AnyObject, NSObjectProtocol, ProtocolObject};
    use objc2::{
        ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    };
    use objc2_app_kit::{
        NSAccessibility, NSAccessibilityAnnouncementKey,
        NSAccessibilityAnnouncementRequestedNotification,
        NSAccessibilityPostNotificationWithUserInfo, NSAccessibilityPriorityKey,
        NSAccessibilityPriorityLevel, NSAutoresizingMaskOptions, NSBox, NSBoxType, NSButton,
        NSColor, NSDragOperation, NSDraggingDestination, NSDraggingInfo, NSPasteboard,
        NSPasteboardType, NSPasteboardTypeFileURL, NSPasteboardTypeString,
        NSPasteboardURLReadingFileURLsOnlyKey, NSText, NSTextView, NSTitlePosition, NSView,
    };
    use objc2_foundation::{
        NSArray, NSDictionary, NSNumber, NSPoint, NSRect, NSSize, NSString, NSURL,
    };
    use objc2_uniform_type_identifiers::UTType;

    use super::{Accept, Dropped, FileKind, Highlight, Offer, judge};

    /// Width of the [`Highlight`] outline, in points.
    const OUTLINE_WIDTH: f64 = 2.0;
    /// Opacity of the selection tint inside the outline: enough to see, light enough to read
    /// the content through.
    const TINT_ALPHA: f64 = 0.12;

    struct Ivars {
        accept: Accept,
        /// `accept.file_types` resolved once. Identifiers the system does not know are left out.
        types: Vec<Retained<UTType>>,
        on_drop: Box<dyn Fn(Dropped)>,
        /// What the drag over the view offers, judged when it entered.
        offer: Cell<Option<Offer>>,
        feedback: Option<Feedback>,
        /// The [`Feedback::Outline`] box, made on the first drag that can be dropped.
        outline: RefCell<Option<Retained<NSBox>>>,
        /// Mouse events pass through to the views below ([`onto_tray`]).
        click_through: bool,
    }

    /// How the view shows that the drag over it can be dropped.
    #[derive(Clone, Copy)]
    enum Feedback {
        Outline(Highlight),
        /// Highlight the button the view lies on (a menu bar icon), and announce.
        Button {
            announcement: &'static str,
        },
    }

    impl Feedback {
        fn announcement(self) -> &'static str {
            match self {
                Self::Outline(highlight) => highlight.announcement,
                Self::Button { announcement } => announcement,
            }
        }
    }

    define_class!(
        // SAFETY: NSView has no subclassing requirements. `DropView` only adds the optional
        // NSDraggingDestination methods, with the protocol's signatures; its ivars need no
        // Objective-C cleanup.
        #[unsafe(super(NSView))]
        #[thread_kind = MainThreadOnly]
        #[name = "MacUiDropTarget"]
        #[ivars = Ivars]
        struct DropView;

        impl DropView {
            #[unsafe(method(hitTest:))]
            fn hit_test(&self, point: NSPoint) -> *mut NSView {
                if self.ivars().click_through {
                    // Clicks go to the views below. Drags do not use hitTest:, so they still
                    // find this view.
                    std::ptr::null_mut()
                } else {
                    // SAFETY: NSView's hitTest:, with its argument and return types.
                    unsafe { msg_send![super(self), hitTest: point] }
                }
            }
        }

        unsafe impl NSObjectProtocol for DropView {}

        unsafe impl NSDraggingDestination for DropView {
            #[unsafe(method(draggingEntered:))]
            fn dragging_entered(
                &self,
                sender: &ProtocolObject<dyn NSDraggingInfo>,
            ) -> NSDragOperation {
                let offer = self.judge_drag(sender);
                self.ivars().offer.set(offer);
                let op = operation(offer, sender);
                if op != NSDragOperation::None {
                    self.show_highlight(true);
                }
                op
            }

            #[unsafe(method(draggingUpdated:))]
            fn dragging_updated(
                &self,
                sender: &ProtocolObject<dyn NSDraggingInfo>,
            ) -> NSDragOperation {
                // The modifier keys can change the source's mask while the drag moves.
                let op = operation(self.ivars().offer.get(), sender);
                if op == NSDragOperation::None {
                    self.hide_highlight();
                } else if !self.highlight_shown() {
                    self.show_highlight(false);
                }
                op
            }

            #[unsafe(method(draggingExited:))]
            fn dragging_exited(&self, _sender: Option<&ProtocolObject<dyn NSDraggingInfo>>) {
                self.ivars().offer.set(None);
                self.hide_highlight();
            }

            #[unsafe(method(prepareForDragOperation:))]
            fn prepare_for_drag_operation(
                &self,
                sender: &ProtocolObject<dyn NSDraggingInfo>,
            ) -> bool {
                operation(self.ivars().offer.get(), sender) != NSDragOperation::None
            }

            #[unsafe(method(performDragOperation:))]
            fn perform_drag_operation(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
                let offer = self.ivars().offer.take();
                self.hide_highlight();
                let pasteboard = sender.draggingPasteboard();
                let dropped = autoreleasepool(|_| match offer? {
                    Offer::File => single_file_path(&pasteboard).map(Dropped::File),
                    Offer::Text => pasteboard
                        .stringForType(unsafe { NSPasteboardTypeString })
                        .map(|text| Dropped::Text(text.to_string())),
                });
                match dropped {
                    Some(dropped) => {
                        (self.ivars().on_drop)(dropped);
                        true
                    }
                    None => false,
                }
            }

            #[unsafe(method(concludeDragOperation:))]
            fn conclude_drag_operation(
                &self,
                _sender: Option<&ProtocolObject<dyn NSDraggingInfo>>,
            ) {
                self.ivars().offer.set(None);
                self.hide_highlight();
            }

            #[unsafe(method(draggingEnded:))]
            fn dragging_ended(&self, _sender: &ProtocolObject<dyn NSDraggingInfo>) {
                // Also when the drag was cancelled or dropped elsewhere.
                self.ivars().offer.set(None);
                self.hide_highlight();
            }
        }
    );

    define_class!(
        // SAFETY: NSTextView has no subclassing requirements. The overrides keep NSTextView's
        // signatures, and there are no ivars.
        #[unsafe(super(NSTextView, NSText, NSView))]
        #[thread_kind = MainThreadOnly]
        #[name = "MacUiFieldEditorWithoutDrops"]
        struct FieldEditorWithoutDrops;

        impl FieldEditorWithoutDrops {
            #[unsafe(method_id(acceptableDragTypes))]
            fn acceptable_drag_types(&self) -> Retained<NSArray<NSPasteboardType>> {
                NSArray::new()
            }

            #[unsafe(method(updateDragTypeRegistration))]
            fn update_drag_type_registration(&self) {
                self.unregisterDraggedTypes();
            }
        }
    );

    /// A field editor (the text view a window lends the text field being edited) that never
    /// takes drops, so a drag over a field being edited goes on to the [`target`] around it.
    /// Return it, the same one every time, from the window delegate's
    /// `windowWillReturnFieldEditor:toObject:`.
    pub fn field_editor_without_drops(mtm: MainThreadMarker) -> Retained<NSTextView> {
        // SAFETY: NSTextView's `initWithFrame:` initialiser, with a matching argument type.
        let editor: Retained<FieldEditorWithoutDrops> =
            unsafe { msg_send![FieldEditorWithoutDrops::alloc(mtm), initWithFrame: NSRect::ZERO] };
        editor.setFieldEditor(true);
        editor.into_super()
    }

    impl DropView {
        /// Show the [`Feedback`], and tell VoiceOver when `speak` (once per drag that enters).
        fn show_highlight(&self, speak: bool) {
            let Some(feedback) = self.ivars().feedback else {
                return;
            };
            match feedback {
                Feedback::Outline(highlight) => self.show_outline(&highlight),
                Feedback::Button { .. } => {
                    if let Some(button) = self.button_below() {
                        button.highlight(true);
                    }
                }
            }
            if speak {
                announce(self, feedback.announcement());
            }
        }

        /// The button this view lies on, for [`Feedback::Button`].
        fn button_below(&self) -> Option<Retained<NSButton>> {
            // SAFETY: the superview is used right away, while the view hierarchy keeps it alive.
            unsafe { self.superview() }.and_then(|view| view.downcast::<NSButton>().ok())
        }

        fn show_outline(&self, highlight: &Highlight) {
            let mut slot = self.ivars().outline.borrow_mut();
            let outline = slot.get_or_insert_with(|| outline_box(self.mtm(), highlight));
            let bounds = self.bounds();
            let inset = highlight.inset;
            outline.setFrame(NSRect::new(
                NSPoint::new(bounds.origin.x + inset, bounds.origin.y + inset),
                NSSize::new(
                    (bounds.size.width - inset * 2.0).max(0.0),
                    (bounds.size.height - inset * 2.0).max(0.0),
                ),
            ));
            // Added last, so it is above everything the app put in the view.
            self.addSubview(outline);
            outline.setHidden(false);
        }

        fn highlight_shown(&self) -> bool {
            match self.ivars().feedback {
                Some(Feedback::Outline(_)) => self
                    .ivars()
                    .outline
                    .borrow()
                    .as_ref()
                    .is_some_and(|outline| !outline.isHidden()),
                Some(Feedback::Button { .. }) => self
                    .button_below()
                    .is_some_and(|button| button.isHighlighted()),
                None => true,
            }
        }

        fn hide_highlight(&self) {
            match self.ivars().feedback {
                Some(Feedback::Outline(_)) => {
                    if let Some(outline) = self.ivars().outline.borrow().as_ref() {
                        outline.setHidden(true);
                    }
                }
                Some(Feedback::Button { .. }) => {
                    if let Some(button) = self.button_below() {
                        button.highlight(false);
                    }
                }
                None => {}
            }
        }

        fn judge_drag(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> Option<Offer> {
            let ivars = self.ivars();
            let pasteboard = sender.draggingPasteboard();
            autoreleasepool(|_| {
                let files: Vec<FileKind> = file_urls(&pasteboard)
                    .iter()
                    .map(|url| file_kind(url, &ivars.types))
                    .collect();
                let has_text = has_type(&pasteboard, unsafe { NSPasteboardTypeString });
                judge(&files, has_text, ivars.accept)
            })
        }
    }

    /// A view that takes drops as described in the [module docs](super), outlined while a drag
    /// that can be dropped is over it when `highlight` is given. `on_drop` runs on the main
    /// thread, once per accepted drop.
    pub fn target(
        mtm: MainThreadMarker,
        accept: Accept,
        highlight: Option<Highlight>,
        on_drop: impl Fn(Dropped) + 'static,
    ) -> Retained<NSView> {
        drop_view(
            mtm,
            accept,
            highlight.map(Feedback::Outline),
            false,
            Box::new(on_drop),
        )
        .into_super()
    }

    /// Take drops on the menu bar icon of `tray`, as described in the [module docs](super):
    /// while a drag that can be dropped is over the icon, it is highlighted like an open menu and
    /// VoiceOver says `announcement`. `on_drop` runs on the main thread, once per accepted drop;
    /// the app is not active then, so activate it to show anything. Clicks, the menu and the
    /// tooltip work as before. False when there is no icon button (not on the main thread, or
    /// the icon is not shown). Hiding the icon (`set_visible(false)`) drops its button, so call
    /// this again after showing it.
    pub fn onto_tray(
        tray: &tray_icon::TrayIcon,
        accept: Accept,
        announcement: &'static str,
        on_drop: impl Fn(Dropped) + 'static,
    ) -> bool {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        let Some(button) = tray.ns_status_item().and_then(|item| item.button(mtm)) else {
            return false;
        };
        let view = drop_view(
            mtm,
            accept,
            Some(Feedback::Button { announcement }),
            true,
            Box::new(on_drop),
        );
        view.setFrame(button.bounds());
        view.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        // On top of the button and of the tray icon's own click view; click-through.
        button.addSubview(&view);
        true
    }

    fn drop_view(
        mtm: MainThreadMarker,
        accept: Accept,
        feedback: Option<Feedback>,
        click_through: bool,
        on_drop: Box<dyn Fn(Dropped)>,
    ) -> Retained<DropView> {
        let types = accept
            .file_types
            .iter()
            .filter_map(|id| UTType::typeWithIdentifier(&NSString::from_str(id)))
            .collect();
        let this = DropView::alloc(mtm).set_ivars(Ivars {
            accept,
            types,
            on_drop,
            offer: Cell::new(None),
            feedback,
            outline: RefCell::new(None),
            click_through,
        });
        // SAFETY: NSView's designated initialiser, with a matching argument type.
        let view: Retained<DropView> =
            unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] };
        let mut registered: Vec<&NSString> = Vec::new();
        if !accept.file_types.is_empty() {
            registered.push(unsafe { NSPasteboardTypeFileURL });
        }
        if accept.text {
            registered.push(unsafe { NSPasteboardTypeString });
        }
        view.registerForDraggedTypes(&NSArray::from_slice(&registered));
        view
    }

    /// The [`Highlight`] outline: accent border, faint selection tint, no title, hidden from
    /// accessibility (the announcement speaks for it).
    fn outline_box(mtm: MainThreadMarker, highlight: &Highlight) -> Retained<NSBox> {
        let outline = NSBox::initWithFrame(NSBox::alloc(mtm), NSRect::ZERO);
        outline.setBoxType(NSBoxType::Custom);
        outline.setTitlePosition(NSTitlePosition::NoTitle);
        outline.setContentViewMargins(NSSize::new(0.0, 0.0));
        outline.setCornerRadius(highlight.corner_radius);
        outline.setBorderWidth(OUTLINE_WIDTH);
        outline.setBorderColor(&NSColor::controlAccentColor());
        outline.setFillColor(
            &NSColor::selectedContentBackgroundColor().colorWithAlphaComponent(TINT_ALPHA),
        );
        outline.setAccessibilityElement(false);
        outline.setHidden(true);
        outline
    }

    /// Have VoiceOver say `text` right away.
    fn announce(element: &NSView, text: &str) {
        let text = NSString::from_str(text);
        let priority = NSNumber::numberWithInteger(NSAccessibilityPriorityLevel::High.0);
        let text: &AnyObject = &text;
        let priority: &AnyObject = &priority;
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

    /// The operation to show for `offer`: copy, or generic while ⌘ narrows the source's mask
    /// to it. The source is never moved or deleted.
    fn operation(
        offer: Option<Offer>,
        sender: &ProtocolObject<dyn NSDraggingInfo>,
    ) -> NSDragOperation {
        if offer.is_none() {
            return NSDragOperation::None;
        }
        let mask = sender.draggingSourceOperationMask();
        if mask.contains(NSDragOperation::Copy) {
            NSDragOperation::Copy
        } else if mask.contains(NSDragOperation::Generic) {
            NSDragOperation::Generic
        } else {
            NSDragOperation::None
        }
    }

    fn has_type(pasteboard: &NSPasteboard, kind: &NSString) -> bool {
        pasteboard
            .availableTypeFromArray(&NSArray::from_slice(&[kind]))
            .is_some()
    }

    /// The drag's file URLs (no other URLs). Reading the URLs is what gives a sandboxed app
    /// access to the files.
    fn file_urls(pasteboard: &NSPasteboard) -> Vec<Retained<NSURL>> {
        let classes: Retained<NSArray<AnyClass>> = NSArray::from_slice(&[NSURL::class()]);
        let yes = NSNumber::numberWithBool(true);
        let yes: &AnyObject = &yes;
        let options =
            NSDictionary::from_slices(&[unsafe { NSPasteboardURLReadingFileURLsOnlyKey }], &[yes]);
        // SAFETY: NSURL reads from the pasteboard, and the option takes an NSNumber boolean.
        let objects = unsafe { pasteboard.readObjectsForClasses_options(&classes, Some(&options)) };
        objects
            .map(|objects| {
                objects
                    .iter()
                    .filter_map(|object| object.downcast::<NSURL>().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn single_file_path(pasteboard: &NSPasteboard) -> Option<PathBuf> {
        let urls = file_urls(pasteboard);
        let [url] = urls.as_slice() else {
            return None;
        };
        Some(PathBuf::from(url.path()?.to_string()))
    }

    fn file_kind(url: &NSURL, types: &[Retained<UTType>]) -> FileKind {
        if url.hasDirectoryPath() {
            return FileKind::Directory;
        }
        let Some(extension) = url.pathExtension().filter(|ext| ext.length() > 0) else {
            return FileKind::Unknown;
        };
        match UTType::typeWithFilenameExtension(&extension) {
            Some(kind) if !kind.isDynamic() => {
                if types.iter().any(|accepted| kind.conformsToType(accepted)) {
                    FileKind::Accepted
                } else {
                    FileKind::Other
                }
            }
            _ => FileKind::Unknown,
        }
    }

    #[cfg(test)]
    mod tests {
        use objc2::{ClassType, sel};

        use super::{DropView, FieldEditorWithoutDrops};

        #[test]
        fn the_class_registers_with_the_drag_methods() {
            // Registering checks the method signatures against NSDraggingDestination (debug).
            let class = DropView::class();
            for method in [
                sel!(draggingEntered:),
                sel!(draggingUpdated:),
                sel!(draggingExited:),
                sel!(prepareForDragOperation:),
                sel!(performDragOperation:),
                sel!(concludeDragOperation:),
                sel!(draggingEnded:),
                sel!(hitTest:),
            ] {
                assert!(class.instance_method(method).is_some(), "{method:?}");
            }
        }

        #[test]
        fn the_field_editor_overrides_its_drag_registration() {
            let class = FieldEditorWithoutDrops::class();
            assert!(class.instance_method(sel!(acceptableDragTypes)).is_some());
            assert!(
                class
                    .instance_method(sel!(updateDragTypeRegistration))
                    .is_some()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_FILES: Accept = Accept {
        file_types: &["public.text"],
        text: true,
    };

    #[test]
    fn one_accepted_or_unknown_file_drops() {
        assert_eq!(
            judge(&[FileKind::Accepted], false, TEXT_FILES),
            Some(Offer::File)
        );
        assert_eq!(
            judge(&[FileKind::Unknown], true, TEXT_FILES),
            Some(Offer::File)
        );
    }

    #[test]
    fn folders_other_types_and_several_files_do_not() {
        assert_eq!(judge(&[FileKind::Directory], false, TEXT_FILES), None);
        assert_eq!(judge(&[FileKind::Other], false, TEXT_FILES), None);
        assert_eq!(
            judge(&[FileKind::Accepted, FileKind::Accepted], false, TEXT_FILES),
            None
        );
    }

    #[test]
    fn a_file_drag_never_falls_back_to_its_text() {
        // Finder puts the file name on the drag as text too.
        assert_eq!(judge(&[FileKind::Other], true, TEXT_FILES), None);
        assert_eq!(
            judge(&[FileKind::Accepted, FileKind::Unknown], true, TEXT_FILES),
            None
        );
    }

    #[test]
    fn text_drops_only_when_taken() {
        assert_eq!(judge(&[], true, TEXT_FILES), Some(Offer::Text));
        assert_eq!(judge(&[], false, TEXT_FILES), None);
        let files_only = Accept {
            text: false,
            ..TEXT_FILES
        };
        assert_eq!(judge(&[], true, files_only), None);
    }

    #[test]
    fn no_file_types_takes_no_files() {
        let text_only = Accept {
            file_types: &[],
            text: true,
        };
        assert_eq!(judge(&[FileKind::Accepted], true, text_only), None);
        assert_eq!(judge(&[FileKind::Unknown], false, text_only), None);
    }
}
