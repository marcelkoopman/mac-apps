//! Layer helpers shared by the always-built modules and `widgets`.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::NSView;

/// The view's superview, if it has one.
pub fn superview(view: &NSView) -> Option<Retained<NSView>> {
    // SAFETY: AppKit returns the superview or nil; the `Retained` keeps it alive for the caller.
    unsafe { view.superview() }
}

/// Round the view's corners through its layer and clip its content to them.
pub fn round_view(view: &NSView, radius: f64) {
    view.setWantsLayer(true);
    // SAFETY: `layer` returns the view's CALayer or nil; both setters exist on CALayer.
    unsafe {
        let layer: *mut AnyObject = msg_send![view, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setCornerRadius: radius];
            let _: () = msg_send![layer, setMasksToBounds: true];
        }
    }
}
