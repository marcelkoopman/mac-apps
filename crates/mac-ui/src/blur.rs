//! Gaussian blur as a view content filter, at the same size as the view.
//!
//! `CIGaussianBlur` grows its output by the radius. AppKit then scales that larger image back
//! into the view, so blurred content looks smaller than the sharp content and a click to reveal
//! jumps the size. [`gaussian`] builds a `CIFilter` that clamps, blurs and crops back to the
//! input image's own extent, so the filtered view keeps the size it had.

use std::ptr::NonNull;
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol, NSZone};
use objc2::{AnyThread, DefinedClass, Message, define_class, extern_class, msg_send};
use objc2_app_kit::NSView;
use objc2_foundation::{CopyingHelper, NSArray, NSCopying, NSRect, NSString};

#[link(name = "CoreImage", kind = "framework")]
unsafe extern "C" {
    static kCIInputRadiusKey: *const AnyObject;
}

extern_class!(
    #[unsafe(super(NSObject))]
    #[name = "CIFilter"]
    struct CIFilter;
);

unsafe impl NSObjectProtocol for CIFilter {}

struct Input {
    radius: f64,
    /// `inputImage` while a view is being filtered. `+1` while set.
    image: Option<NonNull<AnyObject>>,
}

struct BlurIvars {
    input: Mutex<Input>,
}

impl Drop for BlurIvars {
    fn drop(&mut self) {
        let mut input = self.input.lock().unwrap_or_else(|err| err.into_inner());
        release_image(&mut input.image);
    }
}

// A blur whose output extent is the input extent. AppKit copies a content filter per view;
// the copy carries the radius and receives its own `inputImage` at draw time.
define_class!(
    // SAFETY: `CIFilter` is subclassed by overriding `outputImage` and the input keys, and by
    // implementing `copyWithZone:` so a layer can copy the filter.
    #[unsafe(super(CIFilter))]
    #[name = "MacUiSameSizeBlur"]
    #[ivars = BlurIvars]
    struct SameSizeBlur;

    impl SameSizeBlur {
        #[unsafe(method(setDefaults))]
        fn set_defaults(&self) {}

        #[unsafe(method(inputKeys))]
        fn input_keys(&self) -> *mut AnyObject {
            let image = NSString::from_str("inputImage");
            let radius = NSString::from_str("inputRadius");
            autoreleased(NSArray::from_slice(&[&*image, &*radius]))
        }

        #[unsafe(method(setInputImage:))]
        fn set_input_image(&self, image: Option<&AnyObject>) {
            self.store_image(image);
        }

        #[unsafe(method(inputImage))]
        fn input_image(&self) -> *mut AnyObject {
            self.retained_input()
                .map(autoreleased)
                .unwrap_or(std::ptr::null_mut())
        }

        #[unsafe(method(setInputRadius:))]
        fn set_input_radius(&self, radius: Option<&AnyObject>) {
            self.write_radius(radius);
        }

        #[unsafe(method(setValue:forKey:))]
        fn set_value(&self, value: Option<&AnyObject>, key: &NSString) {
            if key.isEqualToString(&NSString::from_str("inputImage")) {
                self.store_image(value);
            } else if key.isEqualToString(&NSString::from_str("inputRadius")) {
                self.write_radius(value);
            } else {
                let _: () = unsafe { msg_send![super(self), setValue: value, forKey: key] };
            }
        }

        #[unsafe(method(outputImage))]
        fn output_image(&self) -> *mut AnyObject {
            self.render().map(autoreleased).unwrap_or(std::ptr::null_mut())
        }
    }

    unsafe impl NSObjectProtocol for SameSizeBlur {}

    unsafe impl NSCopying for SameSizeBlur {
        #[unsafe(method_id(copyWithZone:))]
        fn copy_with_zone(&self, _zone: *const NSZone) -> Retained<Self> {
            Self::new(self.with_input(|input| input.radius))
        }
    }
);

unsafe impl CopyingHelper for SameSizeBlur {
    type Result = Self;
}

impl SameSizeBlur {
    fn new(radius: f64) -> Retained<Self> {
        let this = Self::alloc().set_ivars(BlurIvars {
            input: Mutex::new(Input {
                radius,
                image: None,
            }),
        });
        unsafe { msg_send![super(this), init] }
    }

    fn write_radius(&self, radius: Option<&AnyObject>) {
        let Some(radius) = radius else {
            return;
        };
        // SAFETY: `inputRadius` is an `NSNumber`. `doubleValue` is its value.
        let radius: f64 = unsafe { msg_send![radius, doubleValue] };
        self.with_input(|input| input.radius = radius);
    }

    /// Clamp, blur, then crop to the input extent so the result is not scaled into the view.
    fn render(&self) -> Option<Retained<AnyObject>> {
        let image = self.retained_input()?;
        let radius = self.with_input(|input| input.radius);
        // SAFETY: `CIImage` methods. Clamp stops the blur sampling transparent pixels outside
        // the image (that fringe is what makes the result look inset). The crop is the input's
        // own extent, so the output is the same size AppKit handed us.
        unsafe {
            let clamped: *mut AnyObject = msg_send![&*image, imageByClampingToExtent];
            let clamped = Retained::retain_autoreleased(clamped)?;
            let blurred: *mut AnyObject =
                msg_send![&*clamped, imageByApplyingGaussianBlurWithSigma: radius];
            let blurred = Retained::retain_autoreleased(blurred)?;
            let extent: NSRect = msg_send![&*image, extent];
            let cropped: *mut AnyObject = msg_send![&*blurred, imageByCroppingToRect: extent];
            Retained::retain_autoreleased(cropped)
        }
    }

    fn with_input<T>(&self, body: impl FnOnce(&mut Input) -> T) -> T {
        let mut input = self.ivars().input.lock().unwrap_or_else(|err| err.into_inner());
        body(&mut input)
    }

    fn store_image(&self, image: Option<&AnyObject>) {
        self.with_input(|input| {
            release_image(&mut input.image);
            let Some(image) = image else {
                return;
            };
            let retained = image.retain();
            input.image = NonNull::new(Retained::into_raw(retained));
        });
    }

    fn retained_input(&self) -> Option<Retained<AnyObject>> {
        self.with_input(|input| {
            let ptr = input.image?;
            // SAFETY: `ptr` is a +1 `CIImage` stored for this filter. The extra retain keeps it
            // alive after the mutex is released.
            unsafe { Retained::retain(ptr.as_ptr()) }
        })
    }
}

/// `outputImage` and `inputKeys` return an autoreleased object.
fn autoreleased(object: Retained<impl Message>) -> *mut AnyObject {
    let ptr = Retained::into_raw(object);
    // SAFETY: `autorelease` takes the +1 from `into_raw` and keeps the object alive for the
    // caller until the current pool drains. That is the return convention of these getters.
    unsafe { msg_send![ptr, autorelease] }
}

fn release_image(slot: &mut Option<NonNull<AnyObject>>) {
    let Some(ptr) = slot.take() else {
        return;
    };
    // SAFETY: the slot held the +1 retain from [`SameSizeBlur::store_image`].
    drop(unsafe { Retained::from_raw(ptr.as_ptr()) });
}

/// A blur at `radius` points whose output has the same extent as its input, or `None` when
/// Core Image cannot build it.
pub fn gaussian(radius: f64) -> Option<Retained<AnyObject>> {
    // Referencing the key keeps the Core Image link.
    let key = unsafe { kCIInputRadiusKey };
    if key.is_null() || AnyClass::get(c"CIImage").is_none() {
        return None;
    }
    let filter = SameSizeBlur::new(radius);
    // SAFETY: `SameSizeBlur` is an Objective-C object, so it is an `AnyObject`.
    Some(unsafe { Retained::cast_unchecked(filter) })
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
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::{MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::NSView;
    use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString};

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

    /// A plain `CIGaussianBlur` grows past the input. The filter we install crops back to it,
    /// which is what keeps a blurred view the same size as the sharp one.
    #[test]
    fn blurred_output_keeps_the_input_extent() {
        let image = finite_image(40.0, 24.0);
        let input = extent_of(&image);
        let filter = gaussian(8.0).expect("filter");
        let key = NSString::from_str("inputImage");
        let _: () = unsafe { msg_send![&*filter, setValue: &*image, forKey: &*key] };
        let output: *mut AnyObject = unsafe { msg_send![&*filter, outputImage] };
        let output = unsafe { Retained::retain_autoreleased(output) }.expect("output");
        let output = extent_of(&output);
        let via_key: *mut AnyObject = unsafe {
            msg_send![&*filter, valueForKey: &*NSString::from_str("outputImage")]
        };
        let via_key = unsafe { Retained::retain_autoreleased(via_key) }.expect("valueForKey");
        let via_key = extent_of(&via_key);
        assert_eq!(via_key.size.width, input.size.width);
        assert_eq!(via_key.size.height, input.size.height);
        assert_eq!(output.size.width, input.size.width);
        assert_eq!(output.size.height, input.size.height);
        assert_eq!(output.origin.x, input.origin.x);
        assert_eq!(output.origin.y, input.origin.y);

        // A layer copies the filter before drawing. The copy must keep the radius and the extent.
        let copied: *mut AnyObject = unsafe { msg_send![&*filter, copy] };
        let copied = unsafe { Retained::from_raw(copied) }.expect("copy");
        let _: () = unsafe { msg_send![&*copied, setValue: &*image, forKey: &*key] };
        let copied_out: *mut AnyObject = unsafe { msg_send![&*copied, outputImage] };
        let copied_out = unsafe { Retained::retain_autoreleased(copied_out) }.expect("copy output");
        let copied_out = extent_of(&copied_out);
        assert_eq!(copied_out.size.width, input.size.width);
        assert_eq!(copied_out.size.height, input.size.height);

        let plain = stock_gaussian(8.0);
        let _: () = unsafe { msg_send![&*plain, setValue: &*image, forKey: &*key] };
        let grown: *mut AnyObject = unsafe { msg_send![&*plain, outputImage] };
        let grown = unsafe { Retained::retain_autoreleased(grown) }.expect("stock output");
        let grown = extent_of(&grown);
        assert!(grown.size.width > input.size.width);
        assert!(grown.size.height > input.size.height);
    }

    fn finite_image(width: f64, height: f64) -> Retained<AnyObject> {
        let color_cls = AnyClass::get(c"CIColor").expect("CIColor");
        let color: *mut AnyObject = unsafe {
            msg_send![color_cls, colorWithRed: 0.2, green: 0.3, blue: 0.4, alpha: 1.0]
        };
        let color = unsafe { Retained::retain_autoreleased(color) }.expect("color");
        let image_cls = AnyClass::get(c"CIImage").expect("CIImage");
        let image: *mut AnyObject = unsafe { msg_send![image_cls, imageWithColor: &*color] };
        let image = unsafe { Retained::retain_autoreleased(image) }.expect("image");
        let rect = NSRect::new(NSPoint::new(3.0, 5.0), NSSize::new(width, height));
        let cropped: *mut AnyObject = unsafe { msg_send![&*image, imageByCroppingToRect: rect] };
        unsafe { Retained::retain_autoreleased(cropped) }.expect("cropped")
    }

    fn extent_of(image: &AnyObject) -> NSRect {
        unsafe { msg_send![image, extent] }
    }

    fn stock_gaussian(radius: f64) -> Retained<AnyObject> {
        let cls = AnyClass::get(c"CIFilter").expect("CIFilter");
        let name = NSString::from_str("CIGaussianBlur");
        let filter: *mut AnyObject = unsafe { msg_send![cls, filterWithName: &*name] };
        let filter = unsafe { Retained::retain_autoreleased(filter) }.expect("CIGaussianBlur");
        let _: () = unsafe { msg_send![&*filter, setDefaults] };
        let number_cls = AnyClass::get(c"NSNumber").expect("NSNumber");
        let number: *mut AnyObject =
            unsafe { msg_send![number_cls, numberWithDouble: radius] };
        let number = unsafe { Retained::retain_autoreleased(number) }.expect("radius");
        let key = NSString::from_str("inputRadius");
        let _: () = unsafe { msg_send![&*filter, setValue: &*number, forKey: &*key] };
        filter
    }
}
