//! Drop target: a view that takes one dropped file, image or text.
//!
//! [`target`] builds an `NSView` registered for file URLs, promised files, image data and/or
//! plain text. Make it the window's content view (or another container) and build the content
//! inside it. AppKit offers a drag to the deepest registered view under the pointer, so plain
//! views, buttons, glass and read-only text and image views inside it leave the drag to the
//! target. Editable text (a field being edited, an editable text view) registers itself and keeps
//! taking its own text drops; give the window [`field_editor_without_drops`] when the whole window
//! should take them instead.
//!
//! While the drag moves it is judged by [`judge`] on what the drag pasteboard declares, without
//! reading any data or file: one file (or one promised file, as Photos and Mail drag them) whose
//! type conforms to one of [`Accept::file_types`], image data when [`Accept::images`] is set, or
//! text when [`Accept::text`] is set. Anything else, including several files, folders and a file
//! of another type, gets `NSDragOperation::None`, so the pointer shows that it cannot drop there.
//! A file whose name maps to no known type is let through: only its contents can tell, so the
//! caller judges it after the drop. The drop itself is read once, on the main thread, and handed
//! to the callback as [`Dropped`]; a promised file is handed over once the source has written it.
//!
//! With a [`Highlight`], a drag that can be dropped outlines the target in the accent colour
//! (system colours only, so it follows the appearance and the user's accent) and VoiceOver
//! announces it.

use std::path::{Path, PathBuf};

/// What a drop delivers to the [`target`] callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dropped {
    /// One file, by path. Reading it is up to the caller; in the App Sandbox the drop has given
    /// the app access to it (no security scope to start or stop).
    File(PathBuf),
    /// One promised file (from Photos, Mail, a picture in Safari, …), which the source wrote
    /// into a new folder of its own in the temporary directory. Read it, then hand the path to
    /// [`discard_promised`].
    Promised(PathBuf),
    /// Encoded image data as the source put it on the drag: PNG, JPEG, HEIC, GIF, WebP, BMP or
    /// TIFF (see [`IMAGE_TYPES`]). The caller owns it, like [`Dropped::Text`].
    Image(Vec<u8>),
    /// The dropped text. The caller owns it, and should move it into a zeroizing wrapper right
    /// away if it can be sensitive.
    Text(String),
}

/// What a [`target`] takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accept {
    /// Uniform type identifiers such as `"public.text"`. A dropped or promised file is taken when
    /// its type conforms to one of them. Empty: no files.
    pub file_types: &'static [&'static str],
    /// Take dropped image data (one of [`IMAGE_TYPES`]) when the drag has no files.
    pub images: bool,
    /// Take dropped plain text (`NSPasteboardTypeString`) when the drag has no files, image data
    /// or promised files.
    pub text: bool,
}

/// The image data types [`Accept::images`] takes, in the order they are preferred: the encoded
/// originals first, TIFF (which apps add as a lowest common format) last.
pub const IMAGE_TYPES: &[&str] = &[
    "public.png",
    "public.jpeg",
    "public.heic",
    "public.heif",
    "com.compuserve.gif",
    "org.webmproject.webp",
    "com.microsoft.bmp",
    "public.tiff",
];

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

/// One file in a drag, as far as its URL (or a promise's declared type) tells before the drop.
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

/// What a drag declares, as [`judge`] sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Contents<'a> {
    /// The drag's file URLs.
    pub files: &'a [FileKind],
    /// The files it promises (one per declared type of each promise).
    pub promised: &'a [FileKind],
    /// It has image data of one of [`IMAGE_TYPES`].
    pub image: bool,
    /// It has plain text.
    pub text: bool,
}

/// What a drag will deliver if it is dropped, see [`judge`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    File,
    Promised,
    Image,
    Text,
}

/// Whether a drag declaring `drag` can be dropped under `accept`, and as what.
///
/// A drag with files is judged by its files only: exactly one, not a directory, of an accepted
/// or unknown type. Its text never stands in, because for a file drag that is just the file's
/// name or path. Without files, image data comes first (a picture dragged from a web page also
/// promises its file and carries its address as text), then promised files (judged like files,
/// and only when the target takes files), then text.
pub fn judge(drag: &Contents<'_>, accept: Accept) -> Option<Offer> {
    let one_file = |kinds: &[FileKind]| {
        matches!(kinds, [FileKind::Accepted | FileKind::Unknown]) && !accept.file_types.is_empty()
    };
    if !drag.files.is_empty() {
        return one_file(drag.files).then_some(Offer::File);
    }
    if drag.image && accept.images {
        return Some(Offer::Image);
    }
    // Promises are files: a target that takes no files does not look at them.
    if !drag.promised.is_empty() && !accept.file_types.is_empty() {
        return one_file(drag.promised).then_some(Offer::Promised);
    }
    (drag.text && accept.text).then_some(Offer::Text)
}

/// Start of the name of the folder a promised file is written into, in the temporary directory.
const PROMISE_FOLDER: &str = "mac-ui-drop-";

/// A new, empty folder for one promised file, in the temporary directory.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn promise_folder() -> Option<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let folder =
        std::env::temp_dir().join(format!("{PROMISE_FOLDER}{}-{nanos}", std::process::id()));
    std::fs::create_dir(&folder).ok()?;
    Some(folder)
}

/// Remove the folder of a [`Dropped::Promised`] file, with the file in it. Does nothing for any
/// other path, so it is safe to call with whatever the caller has.
pub fn discard_promised(path: &Path) {
    let Some(folder) = path.parent() else {
        return;
    };
    if is_promise_folder(folder) {
        let _ = std::fs::remove_dir_all(folder);
    }
}

fn is_promise_folder(folder: &Path) -> bool {
    let named = folder
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(PROMISE_FOLDER));
    // Compared resolved: AppKit can hand back /private/var/… for a temporary directory of
    // /var/….
    let in_temp = folder
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .zip(std::env::temp_dir().canonicalize().ok())
        .is_some_and(|(parent, temp)| parent == temp);
    named && in_temp
}

#[cfg(target_os = "macos")]
pub use appkit::{field_editor_without_drops, target};

#[cfg(target_os = "macos")]
mod appkit {
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2::rc::{Retained, autoreleasepool};
    use objc2::runtime::{AnyClass, AnyObject, NSObjectProtocol, ProtocolObject};
    use objc2::{
        ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send,
    };
    use objc2_app_kit::{
        NSAccessibility, NSAccessibilityAnnouncementKey,
        NSAccessibilityAnnouncementRequestedNotification,
        NSAccessibilityPostNotificationWithUserInfo, NSAccessibilityPriorityKey,
        NSAccessibilityPriorityLevel, NSBox, NSBoxType, NSColor, NSDragOperation,
        NSDraggingDestination, NSDraggingInfo, NSFilePromiseReceiver, NSPasteboard,
        NSPasteboardType, NSPasteboardTypeFileURL, NSPasteboardTypeString,
        NSPasteboardURLReadingFileURLsOnlyKey, NSText, NSTextView, NSTitlePosition, NSView,
    };
    use objc2_foundation::{
        NSArray, NSDictionary, NSError, NSNumber, NSOperationQueue, NSPoint, NSRect, NSSize,
        NSString, NSURL,
    };
    use objc2_uniform_type_identifiers::UTType;

    use super::{
        Accept, Contents, Dropped, FileKind, Highlight, IMAGE_TYPES, Offer, discard_promised,
        judge, promise_folder,
    };

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
        highlight: Option<Highlight>,
        /// The outline, made on the first drag that can be dropped.
        outline: RefCell<Option<Retained<NSBox>>>,
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
                self.perform(sender)
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
        /// Outline the view over its content when it has a [`Highlight`], and tell VoiceOver
        /// when `speak` (once per drag that enters).
        fn show_highlight(&self, speak: bool) {
            let Some(highlight) = self.ivars().highlight else {
                return;
            };
            let mut slot = self.ivars().outline.borrow_mut();
            let outline = slot.get_or_insert_with(|| outline_box(self.mtm(), &highlight));
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
            if speak {
                announce(self, highlight.announcement);
            }
        }

        fn highlight_shown(&self) -> bool {
            self.ivars()
                .outline
                .borrow()
                .as_ref()
                .is_some_and(|outline| !outline.isHidden())
        }

        fn hide_highlight(&self) {
            if let Some(outline) = self.ivars().outline.borrow().as_ref() {
                outline.setHidden(true);
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
                let promised: Vec<FileKind> = if ivars.accept.file_types.is_empty() {
                    Vec::new()
                } else {
                    promises(&pasteboard)
                        .iter()
                        .flat_map(|promise| promised_kinds(promise, &ivars.types))
                        .collect()
                };
                let drag = Contents {
                    files: &files,
                    promised: &promised,
                    image: ivars.accept.images && image_type(&pasteboard).is_some(),
                    text: has_type(&pasteboard, unsafe { NSPasteboardTypeString }),
                };
                judge(&drag, ivars.accept)
            })
        }

        /// Read the drop it judged when the drag entered, and hand it to the callback.
        fn perform(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            let offer = self.ivars().offer.take();
            self.hide_highlight();
            let pasteboard = sender.draggingPasteboard();
            if offer == Some(Offer::Promised) {
                // Handed over later, once the source has written the file.
                return autoreleasepool(|_| self.receive_promise(&pasteboard));
            }
            let dropped = autoreleasepool(|_| match offer? {
                Offer::File => single_file_path(&pasteboard).map(Dropped::File),
                Offer::Image => image_data(&pasteboard).map(Dropped::Image),
                Offer::Promised => None,
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

        /// Have the one promise on the drag write its file into a new folder, then hand it to
        /// the callback as [`Dropped::Promised`] (on the main queue, so on the main thread).
        fn receive_promise(&self, pasteboard: &NSPasteboard) -> bool {
            let receivers = promises(pasteboard);
            let [receiver] = receivers.as_slice() else {
                return false;
            };
            let Some(folder) = promise_folder() else {
                return false;
            };
            let Some(folder_text) = folder.to_str() else {
                let _ = std::fs::remove_dir(&folder);
                return false;
            };
            let destination =
                NSURL::fileURLWithPath_isDirectory(&NSString::from_str(folder_text), true);
            let view = self.retain();
            let reader = RcBlock::new(move |file: NonNull<NSURL>, error: *mut NSError| {
                // SAFETY: AppKit passes the URL of the file it wrote, valid for this call.
                let file = unsafe { file.as_ref() };
                let path = file.path().map(|path| PathBuf::from(path.to_string()));
                match path {
                    Some(path) if error.is_null() => {
                        (view.ivars().on_drop)(Dropped::Promised(path))
                    }
                    _ => discard_promised(&folder.join("file")),
                }
            });
            // SAFETY: the options dictionary is empty, and the main queue runs the reader on the
            // main thread, where the view and its callback live.
            unsafe {
                receiver.receivePromisedFilesAtDestination_options_operationQueue_reader(
                    &destination,
                    &NSDictionary::new(),
                    &NSOperationQueue::mainQueue(),
                    &reader,
                );
            }
            true
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
            highlight,
            outline: RefCell::new(None),
        });
        // SAFETY: NSView's designated initialiser, with a matching argument type.
        let view: Retained<DropView> =
            unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] };
        let mut registered: Vec<Retained<NSString>> = Vec::new();
        if !accept.file_types.is_empty() {
            registered.push(unsafe { NSPasteboardTypeFileURL }.retain());
            registered.extend(NSFilePromiseReceiver::readableDraggedTypes().iter());
        }
        if accept.images {
            registered.extend(IMAGE_TYPES.iter().map(|kind| NSString::from_str(kind)));
        }
        if accept.text {
            registered.push(unsafe { NSPasteboardTypeString }.retain());
        }
        let registered: Vec<&NSString> = registered.iter().map(|kind| &**kind).collect();
        view.registerForDraggedTypes(&NSArray::from_slice(&registered));
        view.into_super()
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

    /// The drag's file promises.
    fn promises(pasteboard: &NSPasteboard) -> Vec<Retained<NSFilePromiseReceiver>> {
        let classes: Retained<NSArray<AnyClass>> =
            NSArray::from_slice(&[NSFilePromiseReceiver::class()]);
        // SAFETY: NSFilePromiseReceiver reads from the pasteboard; no options.
        let objects = unsafe { pasteboard.readObjectsForClasses_options(&classes, None) };
        objects
            .map(|objects| {
                objects
                    .iter()
                    .filter_map(|object| object.downcast::<NSFilePromiseReceiver>().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// One [`FileKind`] per file `promise` declares, judged by its declared type.
    fn promised_kinds(
        promise: &NSFilePromiseReceiver,
        types: &[Retained<UTType>],
    ) -> Vec<FileKind> {
        let declared = promise.fileTypes();
        if declared.count() == 0 {
            return vec![FileKind::Unknown];
        }
        declared
            .iter()
            .map(|id| match UTType::typeWithIdentifier(&id) {
                Some(kind) if !kind.isDynamic() => {
                    if types.iter().any(|accepted| kind.conformsToType(accepted)) {
                        FileKind::Accepted
                    } else {
                        FileKind::Other
                    }
                }
                _ => FileKind::Unknown,
            })
            .collect()
    }

    /// The first of [`IMAGE_TYPES`] the drag has.
    fn image_type(pasteboard: &NSPasteboard) -> Option<Retained<NSString>> {
        let kinds: Vec<Retained<NSString>> = IMAGE_TYPES
            .iter()
            .map(|kind| NSString::from_str(kind))
            .collect();
        let kinds: Vec<&NSString> = kinds.iter().map(|kind| &**kind).collect();
        pasteboard.availableTypeFromArray(&NSArray::from_slice(&kinds))
    }

    fn image_data(pasteboard: &NSPasteboard) -> Option<Vec<u8>> {
        let kind = image_type(pasteboard)?;
        let bytes = pasteboard.dataForType(&kind)?.to_vec();
        (!bytes.is_empty()).then_some(bytes)
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
        images: false,
        text: true,
    };

    const TEXT_AND_IMAGES: Accept = Accept {
        file_types: &["public.text", "public.image"],
        images: true,
        text: true,
    };

    fn files(files: &[FileKind], text: bool) -> Contents<'_> {
        Contents {
            files,
            text,
            ..Contents::default()
        }
    }

    #[test]
    fn one_accepted_or_unknown_file_drops() {
        assert_eq!(
            judge(&files(&[FileKind::Accepted], false), TEXT_FILES),
            Some(Offer::File)
        );
        assert_eq!(
            judge(&files(&[FileKind::Unknown], true), TEXT_FILES),
            Some(Offer::File)
        );
    }

    #[test]
    fn folders_other_types_and_several_files_do_not() {
        assert_eq!(
            judge(&files(&[FileKind::Directory], false), TEXT_FILES),
            None
        );
        assert_eq!(judge(&files(&[FileKind::Other], false), TEXT_FILES), None);
        assert_eq!(
            judge(
                &files(&[FileKind::Accepted, FileKind::Accepted], false),
                TEXT_FILES
            ),
            None
        );
    }

    #[test]
    fn a_file_drag_never_falls_back_to_its_text_or_image() {
        // Finder puts the file name on the drag as text too.
        assert_eq!(judge(&files(&[FileKind::Other], true), TEXT_FILES), None);
        assert_eq!(
            judge(
                &files(&[FileKind::Accepted, FileKind::Unknown], true),
                TEXT_FILES
            ),
            None
        );
        let with_image = Contents {
            image: true,
            ..files(&[FileKind::Directory], true)
        };
        assert_eq!(judge(&with_image, TEXT_AND_IMAGES), None);
    }

    #[test]
    fn text_drops_only_when_taken() {
        assert_eq!(judge(&files(&[], true), TEXT_FILES), Some(Offer::Text));
        assert_eq!(judge(&files(&[], false), TEXT_FILES), None);
        let files_only = Accept {
            text: false,
            ..TEXT_FILES
        };
        assert_eq!(judge(&files(&[], true), files_only), None);
    }

    #[test]
    fn no_file_types_takes_no_files() {
        let text_only = Accept {
            file_types: &[],
            images: false,
            text: true,
        };
        assert_eq!(judge(&files(&[FileKind::Accepted], true), text_only), None);
        assert_eq!(judge(&files(&[FileKind::Unknown], false), text_only), None);
        let promise = Contents {
            promised: &[FileKind::Accepted],
            ..Contents::default()
        };
        assert_eq!(judge(&promise, text_only), None);
    }

    #[test]
    fn image_data_comes_before_promises_and_text() {
        // A picture dragged from a web page: data, a promised file and its address as text.
        let picture = Contents {
            promised: &[FileKind::Accepted],
            image: true,
            text: true,
            ..Contents::default()
        };
        assert_eq!(judge(&picture, TEXT_AND_IMAGES), Some(Offer::Image));
        assert_eq!(judge(&picture, TEXT_FILES), Some(Offer::Promised));
        let no_images_no_files = Accept {
            file_types: &[],
            images: false,
            text: true,
        };
        assert_eq!(judge(&picture, no_images_no_files), Some(Offer::Text));
    }

    #[test]
    fn one_promised_file_drops_like_a_file() {
        let one = Contents {
            promised: &[FileKind::Accepted],
            text: true,
            ..Contents::default()
        };
        assert_eq!(judge(&one, TEXT_AND_IMAGES), Some(Offer::Promised));
        let two = Contents {
            promised: &[FileKind::Accepted, FileKind::Accepted],
            text: true,
            ..Contents::default()
        };
        assert_eq!(
            judge(&two, TEXT_AND_IMAGES),
            None,
            "never falls back to text"
        );
        let other = Contents {
            promised: &[FileKind::Other],
            ..Contents::default()
        };
        assert_eq!(judge(&other, TEXT_AND_IMAGES), None);
    }

    #[test]
    fn image_data_needs_images_taken() {
        let data = Contents {
            image: true,
            ..Contents::default()
        };
        assert_eq!(judge(&data, TEXT_FILES), None);
        assert_eq!(judge(&data, TEXT_AND_IMAGES), Some(Offer::Image));
    }

    #[test]
    fn a_promise_folder_is_discarded_with_its_file() {
        let folder = promise_folder().expect("folder");
        let file = folder.join("photo.heic");
        std::fs::write(&file, b"x").expect("write");
        discard_promised(&file);
        assert!(!folder.exists());
    }

    #[test]
    fn discard_leaves_other_folders_alone() {
        let folder = std::env::temp_dir().join(format!("mac-ui-not-a-drop-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("folder");
        let file = folder.join("keep.txt");
        std::fs::write(&file, b"x").expect("write");
        discard_promised(&file);
        assert!(file.exists());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
