//! Drop target: a view that takes one dropped file or dropped text.
//!
//! [`target`] builds an `NSView` registered for file URLs and/or plain text. Make it the window's
//! content view (or another container) and build the content inside it. AppKit offers a drag to
//! the deepest registered view under the pointer, so plain views, buttons, glass and read-only
//! text and image views inside it leave the drag to the target. Editable text (a field being
//! edited, an editable text view) registers itself and keeps taking its own text drops.
//!
//! While the drag moves it is judged by [`judge`] on what the drag pasteboard declares, without
//! reading any text or file: one file whose type conforms to one of [`Accept::file_types`], or
//! text when [`Accept::text`] is set. Anything else, including several files, folders, a file of
//! another type or file promises, gets `NSDragOperation::None`, so the pointer shows that it
//! cannot drop there. A file whose name maps to no known type is let through: only its contents
//! can tell, so the caller judges it after the drop. The drop itself is read once, on the main
//! thread, and handed to the callback as [`Dropped`].

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
pub use appkit::target;

#[cfg(target_os = "macos")]
mod appkit {
    use std::cell::Cell;
    use std::path::PathBuf;

    use objc2::rc::{Retained, autoreleasepool};
    use objc2::runtime::{AnyClass, AnyObject, NSObjectProtocol, ProtocolObject};
    use objc2::{
        ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    };
    use objc2_app_kit::{
        NSDragOperation, NSDraggingDestination, NSDraggingInfo, NSPasteboard,
        NSPasteboardTypeFileURL, NSPasteboardTypeString, NSPasteboardURLReadingFileURLsOnlyKey,
        NSView,
    };
    use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSRect, NSString, NSURL};
    use objc2_uniform_type_identifiers::UTType;

    use super::{Accept, Dropped, FileKind, Offer, judge};

    struct Ivars {
        accept: Accept,
        /// `accept.file_types` resolved once. Identifiers the system does not know are left out.
        types: Vec<Retained<UTType>>,
        on_drop: Box<dyn Fn(Dropped)>,
        /// What the drag over the view offers, judged when it entered.
        offer: Cell<Option<Offer>>,
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

        unsafe impl NSObjectProtocol for DropView {}

        unsafe impl NSDraggingDestination for DropView {
            #[unsafe(method(draggingEntered:))]
            fn dragging_entered(
                &self,
                sender: &ProtocolObject<dyn NSDraggingInfo>,
            ) -> NSDragOperation {
                let offer = self.judge_drag(sender);
                self.ivars().offer.set(offer);
                operation(offer, sender)
            }

            #[unsafe(method(draggingUpdated:))]
            fn dragging_updated(
                &self,
                sender: &ProtocolObject<dyn NSDraggingInfo>,
            ) -> NSDragOperation {
                // The modifier keys can change the source's mask while the drag moves.
                operation(self.ivars().offer.get(), sender)
            }

            #[unsafe(method(draggingExited:))]
            fn dragging_exited(&self, _sender: Option<&ProtocolObject<dyn NSDraggingInfo>>) {
                self.ivars().offer.set(None);
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
            }
        }
    );

    impl DropView {
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

    /// A view that takes drops as described in the [module docs](super). `on_drop` runs on the
    /// main thread, once per accepted drop.
    pub fn target(
        mtm: MainThreadMarker,
        accept: Accept,
        on_drop: impl Fn(Dropped) + 'static,
    ) -> Retained<NSView> {
        let types = accept
            .file_types
            .iter()
            .filter_map(|id| UTType::typeWithIdentifier(&NSString::from_str(id)))
            .collect();
        let this = DropView::alloc(mtm).set_ivars(Ivars {
            accept,
            types,
            on_drop: Box::new(on_drop),
            offer: Cell::new(None),
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
        view.into_super()
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

        use super::DropView;

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
            ] {
                assert!(class.instance_method(method).is_some(), "{method:?}");
            }
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
