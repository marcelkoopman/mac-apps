//! The table window's chart (*Chart* on the toolbar, only in a build with Swift Charts): the
//! version shown drawn by Swift Charts ([`crate::macos_chart`]) over the sidebar and grid: one
//! point per row, with two pop-ups above it, the x column and the y column, and *Table* to go
//! back (as Esc). Rust works the chart out on a thread
//! ([`crate::chart_data`]); a result for an older request is dropped. Masked like the grid:
//! the chart is hidden under the cover. Nothing here is logged; closing the window drops the
//! chart and empties the pop-ups. Under a chart over a date column, *Summarize* puts its trend
//! into words ([`super::insight`]).

use super::*;
use crate::chart_data::{self, ChartOutcome, ChartRequest};
use crate::macos_chart::ChartHost;
use mac_ui::objc2_app_kit::{NSBezelStyle, NSCellImagePosition, NSImage, NSPopUpButton};

const ROW_H: f64 = 26.0;
const NOTE_H: f64 = 18.0;
const GAP: f64 = 8.0;
/// The widest a pop-up gets (a long column name is cut short).
const POPUP_MAX_W: f64 = 220.0;

/// The pop-ups, by tag.
const X_TAG: isize = 1;
const Y_TAG: isize = 4;

struct Pane {
    pane: Retained<NSView>,
    x: Retained<NSPopUpButton>,
    y: Retained<NSPopUpButton>,
    back: Retained<NSButton>,
    /// The sample and the skipped values, why there is no chart, or "Making the chart…".
    note: Retained<NSTextField>,
    host: Option<ChartHost>,
}

/// What the pop-ups' items stand for, in order.
#[derive(Default)]
struct Choices {
    xs: Vec<usize>,
    ys: Vec<usize>,
}

thread_local! {
    static PANE: RefCell<Option<Pane>> = const { RefCell::new(None) };
    static CHOICES: RefCell<Choices> = RefCell::new(Choices::default());
    /// The chart is up (instead of the grid).
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static REQUEST: Cell<Option<ChartRequest>> = const { Cell::new(None) };
    /// The number of the last request; an outcome with another one is dropped.
    static GENERATION: Cell<u64> = const { Cell::new(0) };
    /// The version the chart was asked for (table, version, versions, rows).
    static SIGNATURE: Cell<Option<(u64, usize, usize, usize)>> = const { Cell::new(None) };
}

/// Build the pane, hidden, over `frame` of `content` (the sidebar and grid). Call before the
/// shade and cover are added, so they stay on top. Nothing without Swift Charts.
pub(super) fn build(mtm: MainThreadMarker, content: &NSView, frame: NSRect) {
    use NSAutoresizingMaskOptions as A;
    if !crate::macos_chart::AVAILABLE {
        return;
    }
    let pane = NSView::initWithFrame(NSView::alloc(mtm), frame);
    pane.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    pane.setHidden(true);
    let fill = widgets::filled_box(mtm, WELL_RADIUS, &NSColor::windowBackgroundColor());
    fill.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), frame.size));
    fill.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    pane.addSubview(&fill);
    let popup = |tag: isize| {
        let popup =
            NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), NSRect::ZERO, false);
        popup.setTag(tag);
        popup.setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewMaxXMargin]));
        DELEGATE.with(|slot| {
            if let Some(delegate) = slot.borrow().as_ref() {
                mac_ui::actions::connect(&popup, delegate, sel!(tableWindowChartChanged:));
            }
        });
        pane.addSubview(&popup);
        popup
    };
    let (x, y) = (popup(X_TAG), popup(Y_TAG));
    let back = NSButton::initWithFrame(NSButton::alloc(mtm), NSRect::ZERO);
    back.setBezelStyle(NSBezelStyle::Push);
    if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str("tablecells"),
        None,
    ) {
        back.setImage(Some(&image));
        back.setImagePosition(NSCellImagePosition::ImageLeading);
    }
    back.setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewMinXMargin]));
    wire(&back, sel!(tableWindowChartBack:));
    pane.addSubview(&back);
    let note = widgets::label(
        mtm,
        mac_ui::metrics::FONT_CAPTION,
        &crate::appearance::caption_color(),
    );
    note.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    note.setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewWidthSizable]));
    pane.addSubview(&note);
    insight::build(mtm, &pane, insight::Place::Chart);
    content.addSubview(&pane);
    let built = Pane {
        pane,
        x,
        y,
        back,
        note,
        host: None,
    };
    relabel_pane(&built);
    PANE.with(|slot| slot.replace(Some(built)));
}

/// The pane, for moving it with the grid when the version strip comes and goes.
pub(super) fn pane() -> Option<Retained<NSView>> {
    PANE.with(|slot| slot.borrow().as_ref().map(|pane| pane.pane.clone()))
}

/// *Chart* on the toolbar: show the chart, or go back to the table.
pub(super) fn toggle() {
    if ACTIVE.get() {
        leave();
    } else {
        enter();
    }
}

/// Esc: back to the table when the chart is up.
pub(super) fn escape() -> bool {
    if ACTIVE.get() {
        leave();
        true
    } else {
        false
    }
}

fn enter() {
    if PANE.with(|slot| slot.borrow().is_none()) {
        return;
    }
    let started = SHOWN.with(|shown| {
        let shown = shown.borrow();
        let Some(view) = shown.as_ref() else {
            return false;
        };
        if !commands::table_window_chart_on(view) {
            return false;
        }
        let Some(request) = REQUEST
            .get()
            .and_then(|request| request.fitted(&view.chart_columns))
            .or_else(|| ChartRequest::first(&view.chart_columns))
        else {
            return false;
        };
        ACTIVE.set(true);
        SIGNATURE.set(Some(signature_of(view)));
        REQUEST.set(Some(request));
        fill(view, request);
        true
    });
    if !started {
        NSBeep();
        return;
    }
    // One overlay at a time: the profile goes.
    if profile::active() {
        profile::leave();
    }
    PANE.with(|slot| {
        if let Some(pane) = slot.borrow().as_ref() {
            pane.pane.setHidden(false);
        }
    });
    ask();
    apply_mask(masked());
}

/// Back to the table (*Table*, Esc, the chart no longer fits): the chart is dropped.
pub(super) fn leave() {
    ACTIVE.set(false);
    // An outcome still on its way is dropped.
    GENERATION.set(GENERATION.get().wrapping_add(1));
    SIGNATURE.set(None);
    insight::reset(insight::Place::Chart);
    PANE.with(|slot| {
        if let Some(pane) = slot.borrow_mut().as_mut() {
            drop_host(pane);
            pane.pane.setHidden(true);
            widgets::wipe_text_field(&pane.note);
        }
    });
}

/// A pop-up changed: `tag` says which, `index` the item chosen.
pub(super) fn changed(tag: isize, index: isize) {
    let (Some(mut request), Ok(index)) = (REQUEST.get(), usize::try_from(index)) else {
        return;
    };
    CHOICES.with(|choices| {
        let choices = choices.borrow();
        match tag {
            X_TAG => request.x = choices.xs.get(index).copied().unwrap_or(request.x),
            Y_TAG => request.y = choices.ys.get(index).copied().unwrap_or(request.y),
            _ => {}
        }
    });
    let fitted = SHOWN.with(|shown| {
        let shown = shown.borrow();
        let view = shown.as_ref()?;
        let request = request.fitted(&view.chart_columns)?;
        fill(view, request);
        Some(request)
    });
    match fitted {
        Some(request) => {
            REQUEST.set(Some(request));
            ask();
        }
        None => leave(),
    }
}

/// A new view in the window: the chart follows the version shown (once its grid is there),
/// or goes when its columns no longer allow one.
pub(super) fn view_changed(view: &TableWindowView) {
    if !ACTIVE.get() || view.working || view.grid.is_empty() {
        return;
    }
    let signature = signature_of(view);
    if SIGNATURE.get() == Some(signature) {
        return;
    }
    SIGNATURE.set(Some(signature));
    match REQUEST
        .get()
        .and_then(|request| request.fitted(&view.chart_columns))
    {
        Some(request) => {
            REQUEST.set(Some(request));
            fill(view, request);
            ask();
        }
        None => leave(),
    }
}

/// Draw what the chart thread worked out, unless it is for an older request.
pub(super) fn show(outcome: ChartOutcome) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if !ACTIVE.get() || outcome.generation != GENERATION.get() {
        return;
    }
    let lang = crate::locale::lang();
    let hidden = masked();
    // Summarize, under the chart, for a chart over a date column.
    insight::set_trend(outcome.trend.clone());
    PANE.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(pane) = borrowed.as_mut() else {
            return;
        };
        let note = match &outcome.result {
            Ok((json, chart_note)) => {
                let drawn = match &pane.host {
                    Some(host) => {
                        host.view().setFrame(host_frame(&pane.pane));
                        host.update(json)
                    }
                    None => match ChartHost::new(mtm, json) {
                        Some(host) => {
                            use NSAutoresizingMaskOptions as A;
                            let view = host.view();
                            view.setFrame(host_frame(&pane.pane));
                            view.setAutoresizingMask(mask(&[
                                A::ViewWidthSizable,
                                A::ViewHeightSizable,
                            ]));
                            pane.pane.addSubview(view);
                            pane.host = Some(host);
                            true
                        }
                        None => false,
                    },
                };
                if let Some(host) = &pane.host {
                    host.view().setHidden(hidden || !drawn);
                }
                if drawn {
                    chart_note.text(lang)
                } else {
                    crate::locale::t("chart_unavailable").to_string()
                }
            }
            Err(problem) => {
                drop_host(pane);
                problem.message(lang).to_string()
            }
        };
        pane.note.setStringValue(&NSString::from_str(&note));
    });
}

/// Masked: the chart is hidden (under the cover) and the pop-ups are off.
pub(super) fn apply_mask(masked: bool) {
    PANE.with(|slot| {
        if let Some(pane) = slot.borrow().as_ref() {
            if let Some(host) = &pane.host {
                host.view().setHidden(masked);
            }
            for popup in [&pane.x, &pane.y] {
                popup.setEnabled(!masked);
            }
        }
    });
    insight::apply_mask(insight::Place::Chart, masked);
}

/// The labels in the current language; an open chart is worked out again (its labels are
/// Rust's).
pub(super) fn relabel() {
    PANE.with(|slot| {
        if let Some(pane) = slot.borrow().as_ref() {
            relabel_pane(pane);
        }
    });
    insight::relabel(insight::Place::Chart);
    if !ACTIVE.get() {
        return;
    }
    let refilled = SHOWN.with(|shown| {
        let shown = shown.borrow();
        match (shown.as_ref(), REQUEST.get()) {
            (Some(view), Some(request)) => {
                fill(view, request);
                true
            }
            _ => false,
        }
    });
    if refilled {
        ask();
    }
}

/// The window is wiped: back to the table, the pop-ups emptied.
pub(super) fn wipe() {
    leave();
    REQUEST.set(None);
    CHOICES.with(|choices| *choices.borrow_mut() = Choices::default());
    PANE.with(|slot| {
        if let Some(pane) = slot.borrow().as_ref() {
            for popup in [&pane.x, &pane.y] {
                popup.removeAllItems();
            }
        }
    });
}

fn relabel_pane(pane: &Pane) {
    let t = crate::locale::t;
    let s = |text: &str| NSString::from_str(text);
    for (popup, label) in [
        (&pane.x, t("chart_x_column")),
        (&pane.y, t("chart_y_column")),
    ] {
        popup.setAccessibilityLabel(Some(&s(label)));
        popup.setToolTip(Some(&s(label)));
    }
    pane.back.setTitle(&s(t("table_view")));
    pane.back
        .setToolTip(Some(&s(&commands::keyed_tip(t("chart_back_tip"), "Esc"))));
    layout(pane);
}

/// Ask the app for the chart of the request, as a new generation.
fn ask() {
    let Some(request) = REQUEST.get() else {
        return;
    };
    let generation = GENERATION.get().wrapping_add(1);
    GENERATION.set(generation);
    // The series asked for is new: Summarize waits for it.
    insight::reset(insight::Place::Chart);
    PANE.with(|slot| {
        if let Some(pane) = slot.borrow().as_ref() {
            pane.note
                .setStringValue(&NSString::from_str(crate::locale::t("chart_working")));
        }
    });
    launcher::emit(UserEvent::TableWindow(TableWindowEvent::Chart {
        request,
        generation,
    }));
}

/// Fill the two pop-ups for `request` on `view`'s columns and lay them out.
fn fill(view: &TableWindowView, request: ChartRequest) {
    let columns = &view.chart_columns;
    let name = |index: usize| {
        view.columns
            .get(index)
            .map_or(String::new(), |column| column.shown.clone())
    };
    let choices = Choices {
        xs: chart_data::x_choices(columns),
        ys: chart_data::y_choices(columns),
    };
    PANE.with(|slot| {
        let borrowed = slot.borrow();
        let Some(pane) = borrowed.as_ref() else {
            return;
        };
        let xs: Vec<String> = choices.xs.iter().map(|i| name(*i)).collect();
        set_items(
            &pane.x,
            &xs,
            choices.xs.iter().position(|i| *i == request.x),
        );
        let ys: Vec<String> = choices.ys.iter().map(|i| name(*i)).collect();
        set_items(
            &pane.y,
            &ys,
            choices.ys.iter().position(|i| *i == request.y),
        );
        layout(pane);
    });
    CHOICES.with(|slot| *slot.borrow_mut() = choices);
}

/// Replace `popup`'s items with `titles` (duplicates kept) and select `selected`.
fn set_items(popup: &NSPopUpButton, titles: &[String], selected: Option<usize>) {
    popup.removeAllItems();
    if let (Some(menu), Some(mtm)) = (popup.menu(), MainThreadMarker::new()) {
        for title in titles {
            let item = NSMenuItem::new(mtm);
            item.setTitle(&NSString::from_str(title));
            menu.addItem(&item);
        }
    }
    if let Some(index) = selected.and_then(|index| isize::try_from(index).ok()) {
        popup.selectItemAtIndex(index);
    }
}

/// The two pop-ups, left to right along the top; *Table* at the right; the note under them.
fn layout(pane: &Pane) {
    let size = pane.pane.frame().size;
    let top = size.height - PAD - ROW_H;
    let mut x = PAD;
    for popup in [&pane.x, &pane.y] {
        popup.sizeToFit();
        let width = popup.frame().size.width.min(POPUP_MAX_W);
        popup.setFrame(NSRect::new(NSPoint::new(x, top), NSSize::new(width, ROW_H)));
        x += width + GAP;
    }
    pane.back.sizeToFit();
    let back_w = pane.back.frame().size.width;
    pane.back.setFrame(NSRect::new(
        NSPoint::new(size.width - PAD - back_w, top),
        NSSize::new(back_w, ROW_H),
    ));
    pane.note.setFrame(NSRect::new(
        NSPoint::new(PAD, top - NOTE_H - 4.0),
        NSSize::new((size.width - PAD * 2.0).max(0.0), NOTE_H),
    ));
}

/// Where the chart goes in the pane: under the note, above Summarize's bar when it shows.
fn host_frame(pane: &NSView) -> NSRect {
    let size = pane.frame().size;
    let bar = insight::reserved(insight::Place::Chart);
    let height = (size.height - PAD - ROW_H - NOTE_H - 8.0 - PAD - bar).max(0.0);
    NSRect::new(
        NSPoint::new(PAD, PAD + bar),
        NSSize::new((size.width - PAD * 2.0).max(0.0), height),
    )
}

fn drop_host(pane: &mut Pane) {
    if let Some(host) = pane.host.take() {
        host.view().removeFromSuperview();
    }
}

fn signature_of(view: &TableWindowView) -> (u64, usize, usize, usize) {
    (
        view.table_key,
        view.versions.current,
        view.versions.labels.len(),
        view.rows,
    )
}
