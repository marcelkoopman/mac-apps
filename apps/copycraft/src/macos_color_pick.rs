//! ⋯ › New › Color from Screen…: AppKit's colour sampler (the loupe, `NSColorSampler`). The
//! picked colour goes on the clipboard and onto a new card as `#RRGGBB`, like the other New
//! values ([`crate::launcher::UserEvent::ColorPicked`]), so the Colour card shows it. The
//! sampler needs no Screen Recording permission. The colour is never logged.
#![cfg(target_os = "macos")]

use mac_ui::block2::RcBlock;
use mac_ui::objc2_app_kit::{NSColor, NSColorSampler, NSColorSpace};

/// Show the loupe; on the main thread (a menu command).
pub(crate) fn start() {
    let sampler = NSColorSampler::new();
    let handler = RcBlock::new(|color: *mut NSColor| {
        // SAFETY: AppKit passes the picked colour, or nil when the user cancelled, and it lives
        // for the call; it is only read here.
        let Some(color) = (unsafe { color.as_ref() }) else {
            return;
        };
        let Some(srgb) = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace()) else {
            return;
        };
        let text = crate::color::picked_text(
            srgb.redComponent(),
            srgb.greenComponent(),
            srgb.blueComponent(),
            srgb.alphaComponent(),
        );
        crate::launcher::emit(crate::launcher::UserEvent::ColorPicked(
            zeroize::Zeroizing::new(text),
        ));
    });
    // SAFETY: a valid block of the signature AppKit expects; AppKit copies it and keeps the
    // sampler until the session ends, and calls it on the main thread.
    unsafe { sampler.showSamplerWithSelectionHandler(&handler) };
}
