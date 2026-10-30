#![cfg(target_os = "macos")]
//! The Swift Charts view of the table window's chart (`swift/CopyCraftCharts/ChartView.swift`,
//! compiled by `build.rs`): [`ChartHost`] is its one safe wrapper and the only `unsafe` of the
//! chart. It lives in copycraft, not mac-ui: the Swift source and its build are copycraft's
//! (mac-ui is shared with ticker, which has no chart).
//!
//! Without the Swift library (an Intel Mac, no `swiftc` at build time; the `copycraft_charts`
//! cfg is off) [`AVAILABLE`] is false, the table window has no Chart button and [`ChartHost::new`]
//! returns `None`.

use mac_ui::objc2::MainThreadMarker;
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2_app_kit::NSView;

/// The chart can be drawn in this build.
pub const AVAILABLE: bool = cfg!(copycraft_charts);

#[cfg(copycraft_charts)]
mod ffi {
    use std::ffi::c_void;

    unsafe extern "C" {
        pub fn cc_chart_create(bytes: *const u8, len: usize) -> *mut c_void;
        pub fn cc_chart_update(view: *mut c_void, bytes: *const u8, len: usize) -> bool;
        pub fn cc_chart_release(view: *mut c_void);
    }
}

/// The NSHostingView drawing a chart from the JSON of [`crate::chart_data::ChartData::to_json`].
/// Main thread only (it holds an `NSView`). Swift's reference is released when dropped.
#[derive(Debug)]
pub struct ChartHost {
    view: Retained<NSView>,
}

impl ChartHost {
    /// A view drawing the chart in `json`, or `None` when it does not decode or this build has
    /// no charts.
    #[cfg(copycraft_charts)]
    pub fn new(_mtm: MainThreadMarker, json: &str) -> Option<ChartHost> {
        // SAFETY: on the main thread (the marker). `json` is valid for `len` bytes during the
        // call; Swift reads it in place and keeps no pointer to it. The result is null or an
        // NSHostingView (an NSView) Swift retained for us: `retain` takes a reference of our
        // own, and `Drop` hands Swift's back through `cc_chart_release`.
        let view = unsafe {
            let raw = ffi::cc_chart_create(json.as_ptr(), json.len());
            Retained::retain(raw.cast::<NSView>())
        }?;
        Some(ChartHost { view })
    }

    #[cfg(not(copycraft_charts))]
    pub fn new(_mtm: MainThreadMarker, _json: &str) -> Option<ChartHost> {
        None
    }

    pub fn view(&self) -> &NSView {
        &self.view
    }

    /// Draw the chart in `json` instead. False (the old chart stays) when it does not decode.
    #[cfg(copycraft_charts)]
    pub fn update(&self, json: &str) -> bool {
        let raw = Retained::as_ptr(&self.view)
            .cast_mut()
            .cast::<std::ffi::c_void>();
        // SAFETY: `self` holds the NSHostingView from `cc_chart_create` (alive while `self`
        // is) and is main-thread only (not Send). `json` is valid for `len` bytes during the
        // call; Swift reads it in place.
        unsafe { ffi::cc_chart_update(raw, json.as_ptr(), json.len()) }
    }

    #[cfg(not(copycraft_charts))]
    pub fn update(&self, _json: &str) -> bool {
        false
    }
}

#[cfg(copycraft_charts)]
impl Drop for ChartHost {
    fn drop(&mut self) {
        let raw = Retained::as_ptr(&self.view)
            .cast_mut()
            .cast::<std::ffi::c_void>();
        // SAFETY: gives back the one reference `cc_chart_create` returned, once. Our own
        // reference (`view`) is released after this, so the pointer is still alive here.
        unsafe { ffi::cc_chart_release(raw) }
    }
}
