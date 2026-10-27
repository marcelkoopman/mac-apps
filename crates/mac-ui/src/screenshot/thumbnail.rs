//! The thumbnail of a screenshot just taken, in the bottom-right corner for about five seconds
//! (longer while the pointer rests on it). A click calls back (the app opens the picture); a
//! drag hands the PNG to another app. Either way it is gone after.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSControl, NSDragOperation, NSDraggingContext, NSDraggingItem, NSDraggingSession,
    NSDraggingSource, NSEvent, NSFloatingWindowLevel, NSImage, NSImageAlignment, NSImageScaling,
    NSImageView, NSPasteboardItem, NSResponder, NSView, NSWindow, NSWindowSharingType,
};
use objc2_core_foundation::{
    CFBoolean, CFData, CFDictionary, CFNumber, CFRetained, CFType, kCFAllocatorNull,
};
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSArray, NSData, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};
use objc2_image_io::{
    CGImageSource, kCGImageSourceCreateThumbnailFromImageAlways,
    kCGImageSourceCreateThumbnailWithTransform, kCGImageSourceShouldCache,
    kCGImageSourceShouldCacheImmediately, kCGImageSourceThumbnailMaxPixelSize,
};
use zeroize::Zeroizing;

use crate::shot_area::{self, Rect};

/// How long the thumbnail stays, and how often it looks again while the pointer is on it.
const HOLD_SECS: f64 = 5.0;
const RECHECK_SECS: f64 = 1.0;
/// Its longest side, and its distance to the screen's edges, in points.
const MAX_SIDE: f64 = 180.0;
/// Its longest side in pixels as decoded: sharp at 2× (Retina); the full picture is never
/// decoded for it.
const MAX_PIXELS: i64 = 360;
const MARGIN: f64 = 16.0;
const RADIUS: f64 = 8.0;
/// A press that moves further than this is a drag, not a click.
const DRAG_SLOP: f64 = 3.0;

struct Thumb {
    window: Retained<NSWindow>,
    view: Retained<ThumbView>,
    png: Zeroizing<Vec<u8>>,
    on_click: Box<dyn Fn()>,
    down: Option<NSPoint>,
    dragging: bool,
}

thread_local! {
    static THUMB: RefCell<Option<Thumb>> = const { RefCell::new(None) };
    static GENERATION: Cell<u64> = const { Cell::new(0) };
    /// The window last closed, kept until the next close: it may be closed from inside one of
    /// its own view's methods, which must not free the view under it.
    static RETIRED: RefCell<Option<(Retained<NSWindow>, Retained<ThumbView>)>> =
        const { RefCell::new(None) };
}

define_class!(
    // SAFETY: NSImageView has no subclassing requirements. `ThumbView` overrides the mouse
    // methods with NSResponder's signatures and adds NSDraggingSource's, with the protocol's
    // signatures; it has no ivars.
    #[unsafe(super(NSImageView, NSControl, NSView, NSResponder))]
    #[thread_kind = MainThreadOnly]
    #[name = "MacUiShotThumbnail"]
    struct ThumbView;

    unsafe impl NSObjectProtocol for ThumbView {}

    unsafe impl NSDraggingSource for ThumbView {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn source_operation_mask(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn dragging_ended(
            &self,
            _session: &NSDraggingSession,
            _point: NSPoint,
            _operation: NSDragOperation,
        ) {
            close_thumbnail();
        }
    }

    impl ThumbView {
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            with_thumb(|thumb| {
                thumb.down = Some(event.locationInWindow());
                thumb.dragging = false;
            });
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let at = event.locationInWindow();
            let start = with_thumb(|thumb| {
                let down = thumb.down?;
                let moved = (at.x - down.x).abs() > DRAG_SLOP || (at.y - down.y).abs() > DRAG_SLOP;
                (moved && !thumb.dragging).then(|| {
                    thumb.dragging = true;
                    NSData::with_bytes(&thumb.png)
                })
            })
            .flatten();
            if let Some(png) = start {
                self.begin_drag(event, &png);
            }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            let clicked = with_thumb(|thumb| thumb.down.take().is_some() && !thumb.dragging)
                .unwrap_or(false);
            if clicked {
                let on_click = THUMB.with(|slot| {
                    slot.borrow_mut()
                        .as_mut()
                        .map(|thumb| std::mem::replace(&mut thumb.on_click, Box::new(|| {})))
                });
                close_thumbnail();
                if let Some(on_click) = on_click {
                    on_click();
                }
            }
        }
    }
);

impl ThumbView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSImageView's designated initialiser (NSView's) on a fresh allocation.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    fn begin_drag(&self, event: &NSEvent, png: &NSData) {
        let item = NSPasteboardItem::new();
        item.setData_forType(png, &NSString::from_str("public.png"));
        let dragging = NSDraggingItem::initWithPasteboardWriter(
            NSDraggingItem::alloc(),
            ProtocolObject::from_ref(&*item),
        );
        let image: Option<Retained<NSImage>> = self.image();
        let contents: Option<&AnyObject> = image.as_deref().map(|image| image.as_ref());
        // SAFETY: the dragging contents are an NSImage (or nil), as the API asks.
        unsafe { dragging.setDraggingFrame_contents(self.bounds(), contents) };
        let items = NSArray::from_retained_slice(&[dragging]);
        let _ = self.beginDraggingSessionWithItems_event_source(
            &items,
            event,
            ProtocolObject::from_ref(self),
        );
    }
}

fn with_thumb<R>(f: impl FnOnce(&mut Thumb) -> R) -> Option<R> {
    THUMB.with(|slot| slot.borrow_mut().as_mut().map(f))
}

/// The picture in `png` decoded at most [`MAX_PIXELS`] on its longest side (smaller pictures
/// as they are), straight from the bytes: no copy of them, no full-size bitmap kept.
fn small_image(png: &[u8]) -> Option<(CFRetained<CGImage>, f64, f64)> {
    let max = CFNumber::new_i64(MAX_PIXELS);
    let yes = CFBoolean::new(true);
    let no = CFBoolean::new(false);
    let length = isize::try_from(png.len()).ok()?;
    // SAFETY: the CFData points into `png` without owning it (kCFAllocatorNull frees nothing)
    // and, with the image source, lives only inside this block, while `png` is borrowed. The
    // thumbnail is decoded at once (ShouldCacheImmediately) into a bitmap of its own, so the
    // CGImage returned never reads `png` again. The option keys are CFStrings and the values a
    // CFNumber and CFBooleans, as ImageIO documents for them.
    let thumb = unsafe {
        let data = CFData::with_bytes_no_copy(None, png.as_ptr(), length, kCFAllocatorNull)?;
        let keys: [&CFType; 5] = [
            kCGImageSourceThumbnailMaxPixelSize,
            kCGImageSourceCreateThumbnailFromImageAlways,
            kCGImageSourceCreateThumbnailWithTransform,
            kCGImageSourceShouldCache,
            kCGImageSourceShouldCacheImmediately,
        ]
        .map(AsRef::as_ref);
        let values: [&CFType; 5] = [
            max.as_ref(),
            yes.as_ref(),
            yes.as_ref(),
            no.as_ref(),
            yes.as_ref(),
        ];
        let options = CFDictionary::from_slices(&keys, &values);
        let source = CGImageSource::with_data(&data, None)?;
        source.thumbnail_at_index(0, Some(options.as_opaque()))?
    };
    let width = CGImage::width(Some(&thumb)) as f64;
    let height = CGImage::height(Some(&thumb)) as f64;
    Some((thumb, width, height))
}

/// Show `png` as the thumbnail, replacing one still showing. `on_click` runs after a click
/// (the thumbnail is gone by then). Keeps the bytes (moved in, not copied) for a drag,
/// zeroized when it closes.
pub fn show_thumbnail(
    mtm: MainThreadMarker,
    png: Zeroizing<Vec<u8>>,
    on_click: impl Fn() + 'static,
) {
    close_thumbnail();
    let Some((small, width, height)) = small_image(&png) else {
        return;
    };
    let Some(screen) = crate::panel::screen_at(mtm, NSEvent::mouseLocation()) else {
        return;
    };
    // At most MAX_PIXELS a side: a larger picture shows at 2×, a smaller one as it is.
    let size = shot_area::fit(width, height, MAX_SIDE);
    let image =
        NSImage::initWithCGImage_size(NSImage::alloc(), &small, NSSize::new(size.0, size.1));
    let visible = screen.visibleFrame();
    let frame = shot_area::bottom_right(
        &Rect::new(
            visible.origin.x,
            visible.origin.y,
            visible.size.width,
            visible.size.height,
        ),
        size,
        MARGIN,
    );
    let frame = NSRect::new(
        NSPoint::new(frame.x, frame.y),
        NSSize::new(frame.width, frame.height),
    );
    let window: Retained<NSWindow> = crate::panel::borderless(NSWindow::alloc(mtm), frame.size);
    crate::panel::configure_floating(&window);
    window.setMovableByWindowBackground(false);
    window.setLevel(NSFloatingWindowLevel);
    window.setSharingType(NSWindowSharingType::None);
    window.setFrame_display(frame, false);
    let view = ThumbView::new(mtm, NSRect::new(NSPoint::new(0.0, 0.0), frame.size));
    view.setEditable(false);
    view.unregisterDraggedTypes();
    view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
    view.setImageAlignment(NSImageAlignment::AlignCenter);
    view.setImage(Some(&image));
    crate::layer::round_view(&view, RADIUS);
    window.setContentView(Some(&view));
    window.orderFrontRegardless();
    let generation = GENERATION.with(|g| {
        g.set(g.get().wrapping_add(1));
        g.get()
    });
    THUMB.with(|slot| {
        *slot.borrow_mut() = Some(Thumb {
            window,
            view,
            png,
            on_click: Box::new(on_click),
            down: None,
            dragging: false,
        })
    });
    hide_after(generation, HOLD_SECS);
}

fn hide_after(generation: u64, secs: f64) {
    let spawned = std::thread::Builder::new()
        .name("shot-thumbnail".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs_f64(secs));
            crate::actions::run_on_main(move || hide(generation));
        });
    if spawned.is_err() {
        hide(generation);
    }
}

fn hide(generation: u64) {
    if GENERATION.with(Cell::get) != generation {
        return;
    }
    // Not while the pointer rests on it or a drag is under way: look again in a moment.
    let busy = with_thumb(|thumb| {
        let at = NSEvent::mouseLocation();
        let frame = thumb.window.frame();
        let over = Rect::new(
            frame.origin.x,
            frame.origin.y,
            frame.size.width,
            frame.size.height,
        )
        .contains(at.x, at.y);
        over || thumb.dragging || thumb.down.is_some()
    })
    .unwrap_or(false);
    if busy {
        hide_after(generation, RECHECK_SECS);
    } else {
        close_thumbnail();
    }
}

/// The window numbers of the thumbnail showing and the one last closed, for a capture to
/// leave out.
pub(super) fn thumbnail_windows() -> Vec<u32> {
    let showing = THUMB.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|thumb| thumb.window.windowNumber())
    });
    let retired = RETIRED.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|(window, _)| window.windowNumber())
    });
    [showing, retired]
        .into_iter()
        .flatten()
        .filter_map(|number| u32::try_from(number).ok())
        .collect()
}

/// Close the thumbnail and zeroize its copy of the picture (also on Wipe and the lock screen).
pub fn close_thumbnail() {
    GENERATION.with(|g| g.set(g.get().wrapping_add(1)));
    let Some(mut thumb) = THUMB.with(|slot| slot.borrow_mut().take()) else {
        return;
    };
    thumb.view.setImage(None);
    thumb.window.orderOut(None);
    // Zeroizing clears the bytes as they drop, now.
    drop(std::mem::take(&mut thumb.png));
    let Thumb { window, view, .. } = thumb;
    RETIRED.with(|slot| *slot.borrow_mut() = Some((window, view)));
}
