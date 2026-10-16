//! The first page of a document as a picture, from the system's Quick Look thumbnailer.
//!
//! `QLThumbnailGenerator::generateBestRepresentation` renders a Word, Pages, Keynote, … file
//! with the system's own generators (the app has no parser for them). The result is a `CGImage`
//! that is encoded to PNG in memory: nothing is written to disk by this module (the
//! `saveBestRepresentation…` calls are not used). The completion handler runs on a system
//! queue; the caller waits for it on its own thread with a timeout, so call this from a
//! background thread, never the main one.

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::AnyThread;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
use objc2_foundation::{NSDictionary, NSError, NSSize, NSString, NSURL};
use objc2_quick_look_thumbnailing::{
    QLThumbnailGenerationRequest, QLThumbnailGenerationRequestRepresentationTypes,
    QLThumbnailGenerator, QLThumbnailRepresentation, QLThumbnailRepresentationType,
};

/// PNG bytes of the first page of the file at `path`, at most `width` × `height` points at
/// `scale` (so `width * scale` pixels wide at most). `None` when the system has no thumbnail
/// for the file or does not deliver one within `timeout`. Only a real thumbnail counts, never
/// the generic file icon.
pub fn first_page_png(
    path: &Path,
    width: f64,
    height: f64,
    scale: f64,
    timeout: Duration,
) -> Option<Vec<u8>> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
    let (tx, rx) = mpsc::channel::<Option<Vec<u8>>>();
    let handler = RcBlock::new(
        move |rep: *mut QLThumbnailRepresentation, _err: *mut NSError| {
            // SAFETY: `rep` is null or a representation the system keeps alive for the duration of
            // this call; its image is encoded to PNG before the call returns.
            let png = unsafe {
                rep.as_ref()
                    .filter(|rep| rep.r#type() == QLThumbnailRepresentationType::Thumbnail)
                    .and_then(|rep| {
                        let bitmap = NSBitmapImageRep::initWithCGImage(
                            NSBitmapImageRep::alloc(),
                            &rep.CGImage(),
                        );
                        // An empty dictionary is a valid property list for any file type.
                        bitmap.representationUsingType_properties(
                            NSBitmapImageFileType::PNG,
                            &NSDictionary::new(),
                        )
                    })
                    .map(|data| data.to_vec())
            };
            // The receiver is gone after a timeout.
            let _ = tx.send(png);
        },
    );
    // SAFETY: documented Quick Look calls with a valid file URL, a positive size and scale, and a
    // block that matches the completion handler's signature; the request that is cancelled after
    // a timeout is the one submitted here.
    unsafe {
        let generator = QLThumbnailGenerator::sharedGenerator();
        let request =
            QLThumbnailGenerationRequest::initWithFileAtURL_size_scale_representationTypes(
                QLThumbnailGenerationRequest::alloc(),
                &url,
                NSSize::new(width, height),
                scale,
                QLThumbnailGenerationRequestRepresentationTypes::Thumbnail,
            );
        generator.generateBestRepresentationForRequest_completionHandler(&request, &handler);
        match rx.recv_timeout(timeout) {
            Ok(png) => png,
            Err(_) => {
                generator.cancelRequest(&request);
                None
            }
        }
    }
}
