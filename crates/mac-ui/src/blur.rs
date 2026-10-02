//! Gaussian blur as a view content filter (`CIGaussianBlur`).
//!
//! [`gaussian`] builds the filter. [`set_content_filters`] installs filters on a view, or clears
//! them when the array is empty. Callers choose which views to blur and what to show when the
//! filter cannot be created.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_app_kit::NSView;
use objc2_foundation::{NSArray, NSString};

#[link(name = "CoreImage", kind = "framework")]
unsafe extern "C" {
    static kCIInputRadiusKey: *const AnyObject;
}

/// A `CIGaussianBlur` at `radius` points, or `None` when Core Image cannot build it.
pub fn gaussian(radius: f64) -> Option<Retained<AnyObject>> {
    // Referencing the key keeps the Core Image link. The symbol is the filter's radius input.
    let _linked = unsafe { kCIInputRadiusKey };
    let cls = AnyClass::get(c"CIFilter")?;
    let name = NSString::from_str("CIGaussianBlur");
    let filter = unsafe {
        let ptr: *mut AnyObject = msg_send![cls, filterWithName: &*name];
        Retained::retain_autoreleased(ptr)
    }?;
    let _: () = unsafe { msg_send![&*filter, setDefaults] };
    let number_cls = AnyClass::get(c"NSNumber")?;
    let radius = unsafe {
        let ptr: *mut AnyObject = msg_send![number_cls, numberWithDouble: radius];
        Retained::retain_autoreleased(ptr)
    }?;
    let key = unsafe { kCIInputRadiusKey };
    if key.is_null() {
        return None;
    }
    // SAFETY: `key` was checked non-null. It is the process-lifetime `kCIInputRadiusKey` string.
    let key = unsafe { &*key };
    let _: () = unsafe { msg_send![&*filter, setValue: &*radius, forKey: key] };
    Some(filter)
}

/// Install `filters` as the view's content filters.
///
/// Turns on a layer and Core Image filters so a gaussian blur can draw. An empty array clears
/// the blur.
pub fn set_content_filters(view: &NSView, filters: &NSArray<AnyObject>) {
    view.setWantsLayer(true);
    view.setLayerUsesCoreImageFilters(true);
    // SAFETY: `setContentFilters:` is bound only with objc2-core-image. `filters` is empty or
    // holds the `CIFilter` from [`gaussian`].
    let _: () = unsafe { msg_send![view, setContentFilters: filters] };
}

#[cfg(test)]
mod tests {
    use objc2::{MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::NSView;
    use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize};

    use super::{gaussian, set_content_filters};

    #[test]
    fn gaussian_blur_builds() {
        assert!(gaussian(22.0).is_some());
    }

    #[test]
    fn filters_install_and_clear() {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let view = NSView::initWithFrame(
            NSView::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(40.0, 20.0)),
        );
        let filter = gaussian(8.0).expect("filter");
        set_content_filters(&view, &NSArray::from_slice(&[&*filter]));
        set_content_filters(&view, &NSArray::from_slice(&[]));
    }
}
