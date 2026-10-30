//! Gaussian blur as a view content filter (`CIGaussianBlur`).
//!
//! [`gaussian`] builds the filter. [`set_content_filters`] installs filters on a view, or clears
//! them when the array is empty. Callers choose which views to blur and what to show when the
//! filter cannot be created. Typed through objc2-core-image.

use objc2::rc::Retained;
use objc2_app_kit::NSView;
pub use objc2_core_image::CIFilter;
use objc2_core_image::kCIInputRadiusKey;
use objc2_foundation::{NSArray, NSNumber, NSObjectNSKeyValueCoding, NSString};

/// A `CIGaussianBlur` at `radius` points, or `None` when Core Image cannot build it.
pub fn gaussian(radius: f64) -> Option<Retained<CIFilter>> {
    let name = NSString::from_str("CIGaussianBlur");
    let radius = NSNumber::new_f64(radius);
    // SAFETY: `filterWithName:` takes an NSString and returns a CIFilter or nil; `setDefaults`
    // takes nothing. `kCIInputRadiusKey` is an immutable Core Image constant, and the radius
    // input takes an NSNumber, as `setValue:forKey:` gets here.
    unsafe {
        let filter = CIFilter::filterWithName(&name)?;
        filter.setDefaults();
        filter.setValue_forKey(Some(&radius), kCIInputRadiusKey);
        Some(filter)
    }
}

/// Install `filters` as the view's content filters.
///
/// Turns on a layer and Core Image filters so a gaussian blur can draw. An empty array clears
/// the blur.
pub fn set_content_filters(view: &NSView, filters: &NSArray<CIFilter>) {
    view.setWantsLayer(true);
    view.setLayerUsesCoreImageFilters(true);
    view.setContentFilters(filters);
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
