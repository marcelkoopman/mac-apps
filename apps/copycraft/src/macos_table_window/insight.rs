//! *Explain* under the table profile and *Summarize* under the chart: a bar along the bottom of
//! the pane with the button (and, in the profile, the column), *Copy*, a note and the text.
//! The Rust fact sentence is always shown ([`crate::table_insight`]); the Apple model on the
//! device puts the facts into words on a worker thread (`mac_ai::Job`), and its answer is shown
//! above them only when its numbers are the facts' numbers. Without the model the button is
//! off and the note says why. Esc, leaving the pane and closing the window stop a run (the
//! job is cancelled and its outcome dropped). The answer lives only here: never in history,
//! on disk or in a log; *Copy* goes through the app's copy (sensitivity checks).

use std::sync::Arc;

use super::*;
use crate::table_insight::{self, InsightOutcome, InsightResult, InsightText, TrendFacts};
use crate::table_profile::TableProfile;
use mac_ui::objc2_app_kit::{NSBezelStyle, NSCellImagePosition, NSImage};
use zeroize::Zeroizing;

const ROW_H: f64 = 26.0;
const NOTE_H: f64 = 18.0;
const GAP: f64 = 8.0;
/// The text under the buttons: about five lines.
const TEXT_H: f64 = 84.0;
/// The bar's height.
const BAR_H: f64 = ROW_H + GAP + TEXT_H;
const POPUP_MAX_W: f64 = 220.0;

/// Which pane a bar is in. Its number is the tag of its buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Place {
    Profile,
    Chart,
}

impl Place {
    fn index(self) -> usize {
        match self {
            Place::Profile => 0,
            Place::Chart => 1,
        }
    }

    pub(super) fn from_tag(tag: isize) -> Option<Place> {
        match tag {
            0 => Some(Place::Profile),
            1 => Some(Place::Chart),
            _ => None,
        }
    }
}

struct Bar {
    bar: Retained<NSView>,
    /// The number column with outliers to explain (profile only).
    column: Option<Retained<NSPopUpButton>>,
    run: Retained<NSButton>,
    copy: Retained<NSButton>,
    note: Retained<NSTextField>,
    scroll: Retained<NSScrollView>,
    text: Retained<NSTextView>,
}

/// What a bar is about: a profile (one of its columns) or a chart's series.
enum Subject {
    Outliers(Arc<TableProfile>),
    Trend(Arc<TrendFacts>),
}

/// Why there is no answer, for the note.
#[derive(Clone, Copy)]
enum Note {
    LeftOut,
    Failed(mac_ai::Error),
}

#[derive(Default)]
struct State {
    subject: Option<Subject>,
    /// Profile: the places in its columns the pop-up offers, and the one chosen.
    columns: Vec<usize>,
    column: usize,
    answer: Option<Zeroizing<String>>,
    note: Option<Note>,
    /// What the text shows, for *Copy*.
    shown: Option<Zeroizing<String>>,
}

impl State {
    fn sentence(&self, lang: crate::locale::Lang) -> Option<Zeroizing<String>> {
        match self.subject.as_ref()? {
            Subject::Outliers(profile) => {
                let place = *self.columns.get(self.column)?;
                table_insight::outlier_sentence(profile.columns.get(place)?, lang)
            }
            Subject::Trend(trend) => Some(trend.sentence(lang)),
        }
    }

    fn prompt(&self, lang: crate::locale::Lang) -> Option<(mac_ai::FactTask, Zeroizing<String>)> {
        match self.subject.as_ref()? {
            Subject::Outliers(profile) => {
                let place = *self.columns.get(self.column)?;
                let prompt = table_insight::outlier_prompt(profile.columns.get(place)?, lang)?;
                Some((mac_ai::FactTask::ExplainOutliers, prompt))
            }
            Subject::Trend(trend) => Some((mac_ai::FactTask::SummarizeTrend, trend.prompt(lang))),
        }
    }
}

/// The model run on its way.
struct Running {
    run: u64,
    place: Place,
    job: Arc<mac_ai::Job>,
}

thread_local! {
    static BARS: RefCell<[Option<Bar>; 2]> = const { RefCell::new([None, None]) };
    static STATES: RefCell<[State; 2]> = RefCell::new([State::default(), State::default()]);
    static RUNNING: RefCell<Option<Running>> = const { RefCell::new(None) };
    static NEXT_RUN: Cell<u64> = const { Cell::new(0) };
}

/// Build `place`'s bar, hidden, along the bottom of `pane`.
pub(super) fn build(mtm: MainThreadMarker, pane: &NSView, place: Place) {
    use NSAutoresizingMaskOptions as A;
    let size = pane.frame().size;
    let bar = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(
            NSPoint::new(PAD, PAD),
            NSSize::new((size.width - PAD * 2.0).max(0.0), BAR_H),
        ),
    );
    bar.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewMaxYMargin]));
    bar.setHidden(true);
    let tag = place.index() as isize;
    let button = |symbol: &str, action: Sel| {
        let button = NSButton::initWithFrame(NSButton::alloc(mtm), NSRect::ZERO);
        button.setBezelStyle(NSBezelStyle::Push);
        if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(symbol),
            None,
        ) {
            button.setImage(Some(&image));
            button.setImagePosition(NSCellImagePosition::ImageLeading);
        }
        button.setTag(tag);
        button.setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewMaxXMargin]));
        wire(&button, action);
        bar.addSubview(&button);
        button
    };
    let column = (place == Place::Profile).then(|| {
        let popup =
            NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), NSRect::ZERO, false);
        popup.setTag(tag);
        popup.setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewMaxXMargin]));
        DELEGATE.with(|slot| {
            if let Some(delegate) = slot.borrow().as_ref() {
                mac_ui::actions::connect(&popup, delegate, sel!(tableWindowInsightColumn:));
            }
        });
        bar.addSubview(&popup);
        popup
    });
    let run = button("sparkles", sel!(tableWindowInsightRun:));
    let copy = button("doc.on.doc", sel!(tableWindowInsightCopy:));
    let note = widgets::label(
        mtm,
        mac_ui::metrics::FONT_CAPTION,
        &crate::appearance::caption_color(),
    );
    note.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    note.setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewWidthSizable]));
    bar.addSubview(&note);
    let text = widgets::read_only_text_view(mtm, NSSize::new(4.0, 4.0));
    text.setFont(Some(&NSFont::systemFontOfSize(mac_ui::metrics::FONT_BODY)));
    text.setTextColor(Some(&NSColor::labelColor()));
    let scroll = widgets::passive_text_scroll(mtm, &text);
    scroll.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new((size.width - PAD * 2.0).max(0.0), TEXT_H),
    ));
    scroll.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewMaxYMargin]));
    bar.addSubview(&scroll);
    pane.addSubview(&bar);
    let built = Bar {
        bar,
        column,
        run,
        copy,
        note,
        scroll,
        text,
    };
    relabel_bar(&built, place);
    BARS.with(|slot| slot.borrow_mut()[place.index()] = Some(built));
}

/// The height `place`'s bar takes from the bottom of its pane (with the gap above it), 0 when
/// it is hidden.
pub(super) fn reserved(place: Place) -> f64 {
    BARS.with(|slot| {
        slot.borrow()[place.index()]
            .as_ref()
            .filter(|bar| !bar.bar.isHidden())
            .map_or(0.0, |_| BAR_H + GAP)
    })
}

/// The profile shown changed (or went): Explain is about its number columns with outliers.
pub(super) fn set_profile(profile: Option<Arc<TableProfile>>) {
    reset(Place::Profile);
    let Some(profile) = profile else {
        return;
    };
    let columns = table_insight::explainable(&profile);
    if columns.is_empty() {
        return;
    }
    STATES.with(|slot| {
        let state = &mut slot.borrow_mut()[Place::Profile.index()];
        state.columns = columns;
        state.subject = Some(Subject::Outliers(profile));
    });
    paint(Place::Profile);
}

/// The chart shown changed (or went): Summarize is about its points over a date column.
pub(super) fn set_trend(trend: Option<Arc<TrendFacts>>) {
    reset(Place::Chart);
    let Some(trend) = trend else {
        return;
    };
    STATES.with(|slot| {
        slot.borrow_mut()[Place::Chart.index()].subject = Some(Subject::Trend(trend));
    });
    paint(Place::Chart);
}

/// The column pop-up changed.
pub(super) fn column_changed(index: isize) {
    let Ok(index) = usize::try_from(index) else {
        return;
    };
    stop_in(Place::Profile);
    STATES.with(|slot| {
        let state = &mut slot.borrow_mut()[Place::Profile.index()];
        if index < state.columns.len() {
            state.column = index;
        }
        state.answer = None;
        state.note = None;
    });
    paint(Place::Profile);
}

/// *Explain* / *Summarize*: ask the model on a worker thread.
pub(super) fn ask(place: Place) {
    if masked() || running() || mac_ai::availability().is_err() {
        NSBeep();
        return;
    }
    let lang = crate::locale::lang();
    let prompt = STATES.with(|slot| slot.borrow()[place.index()].prompt(lang));
    let Some((task, prompt)) = prompt else {
        NSBeep();
        return;
    };
    let run = NEXT_RUN.get().wrapping_add(1);
    NEXT_RUN.set(run);
    let job = Arc::new(mac_ai::Job::new());
    let worker = Arc::clone(&job);
    let started = std::thread::Builder::new()
        .name("copycraft-insight".into())
        .spawn(move || {
            let result = table_insight::run_and_check(&worker, task, &prompt, lang);
            drop(prompt);
            launcher::emit(UserEvent::TableInsight(InsightOutcome { run, result }));
        });
    STATES.with(|slot| {
        let state = &mut slot.borrow_mut()[place.index()];
        state.answer = None;
        state.note = None;
        if started.is_err() {
            // No log: the note says enough.
            state.note = Some(Note::Failed(mac_ai::Error::Failed));
        }
    });
    if started.is_ok() {
        RUNNING.with(|slot| slot.replace(Some(Running { run, place, job })));
    }
    paint(place);
}

/// The model answered (or did not): shown unless the run was stopped or replaced.
pub(super) fn show(outcome: InsightOutcome) {
    let place = RUNNING.with(|slot| {
        let mut running = slot.borrow_mut();
        match running.as_ref() {
            Some(current) if current.run == outcome.run => running.take().map(|r| r.place),
            _ => None,
        }
    });
    let Some(place) = place else {
        return;
    };
    STATES.with(|slot| {
        let state = &mut slot.borrow_mut()[place.index()];
        match outcome.result {
            InsightResult::Answer(text) => state.answer = Some(text),
            InsightResult::LeftOut => state.note = Some(Note::LeftOut),
            InsightResult::Failed(mac_ai::Error::Cancelled) => {}
            InsightResult::Failed(error) => state.note = Some(Note::Failed(error)),
        }
    });
    paint(place);
}

/// *Copy* under the text: the app copies it (as any copy, without a history entry).
pub(super) fn copy(place: Place) {
    let text = STATES.with(|slot| slot.borrow()[place.index()].shown.clone());
    match text {
        Some(text) if !masked() => {
            launcher::emit(UserEvent::CopyTableInsight(InsightText(text)));
        }
        _ => NSBeep(),
    }
}

/// Esc: stop a run on its way. `false` when none is.
pub(super) fn escape() -> bool {
    let place = RUNNING.with(|slot| slot.borrow().as_ref().map(|running| running.place));
    match place {
        Some(place) => {
            stop_in(place);
            paint(place);
            true
        }
        None => false,
    }
}

/// `place`'s pane goes (or shows something else): its run stops, its text is wiped and the
/// bar hides.
pub(super) fn reset(place: Place) {
    stop_in(place);
    STATES.with(|slot| slot.borrow_mut()[place.index()] = State::default());
    BARS.with(|slot| {
        if let Some(bar) = slot.borrow()[place.index()].as_ref() {
            bar.bar.setHidden(true);
            widgets::wipe_text_view(&bar.text);
            widgets::wipe_text_field(&bar.note);
            if let Some(popup) = &bar.column {
                popup.removeAllItems();
            }
        }
    });
}

/// Masked: the text is hidden (under the cover) and the controls are off.
pub(super) fn apply_mask(place: Place, _masked: bool) {
    paint(place);
}

/// The labels in the current language. An answer (in the old language) goes; the facts are
/// written again.
pub(super) fn relabel(place: Place) {
    stop_in(place);
    STATES.with(|slot| {
        let state = &mut slot.borrow_mut()[place.index()];
        state.answer = None;
        state.note = None;
    });
    BARS.with(|slot| {
        if let Some(bar) = slot.borrow()[place.index()].as_ref() {
            relabel_bar(bar, place);
        }
    });
    paint(place);
}

fn running() -> bool {
    RUNNING.with(|slot| slot.borrow().is_some())
}

/// Stop the run of `place`, if it has one: the job is cancelled, its outcome dropped.
fn stop_in(place: Place) {
    RUNNING.with(|slot| {
        let mut running = slot.borrow_mut();
        if running.as_ref().is_some_and(|r| r.place == place)
            && let Some(stopped) = running.take()
        {
            stopped.job.cancel();
        }
    });
}

/// Show `place`'s state: the bar (when there are facts), the pop-up, the text, the note and
/// which controls work.
fn paint(place: Place) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let lang = crate::locale::lang();
    let hidden = masked();
    let busy = RUNNING.with(|slot| slot.borrow().as_ref().map(|r| r.place)) == Some(place);
    let available = mac_ai::availability();
    let painted = STATES.with(|slot| {
        let mut states = slot.borrow_mut();
        let state = &mut states[place.index()];
        let sentence = state.sentence(lang)?;
        let text = match &state.answer {
            Some(answer) => Zeroizing::new(format!(
                "{}\n\n{}",
                answer.as_str(),
                crate::locale::tf("insight_facts", &[&sentence.as_str()])
            )),
            None => sentence,
        };
        state.shown = Some(text.clone());
        let note = if busy {
            crate::locale::t("insight_working")
        } else {
            match (state.note, available) {
                (Some(Note::LeftOut), _) => crate::locale::t("insight_left_out"),
                (Some(Note::Failed(error)), _) => commands::ai_error_text(error),
                (None, Err(reason)) => commands::ai_reason(reason),
                (None, Ok(())) => "",
            }
        };
        let names: Vec<String> = match &state.subject {
            Some(Subject::Outliers(profile)) => state
                .columns
                .iter()
                .filter_map(|place| profile.columns.get(*place))
                .map(|column| column.name.clone())
                .collect(),
            _ => Vec::new(),
        };
        Some((text, note, names, state.column))
    });
    BARS.with(|slot| {
        let borrowed = slot.borrow();
        let Some(bar) = borrowed[place.index()].as_ref() else {
            return;
        };
        let Some((text, note, mut names, column)) = painted else {
            bar.bar.setHidden(true);
            return;
        };
        bar.bar.setHidden(false);
        if let Some(popup) = &bar.column {
            popup.removeAllItems();
            if let Some(menu) = popup.menu() {
                for name in &names {
                    let item = NSMenuItem::new(mtm);
                    item.setTitle(&NSString::from_str(name));
                    menu.addItem(&item);
                }
            }
            if let Ok(index) = isize::try_from(column) {
                popup.selectItemAtIndex(index);
            }
            popup.setEnabled(!hidden && !busy);
        }
        names.zeroize();
        widgets::wipe_text_view(&bar.text);
        bar.text.setString(&NSString::from_str(&text));
        widgets::fit_text_view(&bar.text, Some(bar.scroll.contentSize().width));
        bar.scroll.setHidden(hidden);
        bar.note.setStringValue(&NSString::from_str(note));
        bar.run.setEnabled(available.is_ok() && !busy && !hidden);
        let tip = match available {
            Err(reason) => commands::ai_reason(reason),
            Ok(()) => match place {
                Place::Profile => crate::locale::t("insight_explain_tip"),
                Place::Chart => crate::locale::t("insight_summarize_tip"),
            },
        };
        bar.run.setToolTip(Some(&NSString::from_str(tip)));
        bar.copy.setEnabled(!hidden);
        layout(bar);
    });
}

fn relabel_bar(bar: &Bar, place: Place) {
    let t = crate::locale::t;
    let s = |text: &str| NSString::from_str(text);
    match place {
        Place::Profile => {
            bar.run.setTitle(&s(t("insight_explain")));
            bar.run.setToolTip(Some(&s(t("insight_explain_tip"))));
        }
        Place::Chart => {
            bar.run.setTitle(&s(t("insight_summarize")));
            bar.run.setToolTip(Some(&s(t("insight_summarize_tip"))));
        }
    }
    bar.copy.setTitle(&s(t("copy")));
    bar.copy.setToolTip(Some(&s(t("insight_copy_tip"))));
    if let Some(popup) = &bar.column {
        popup.setAccessibilityLabel(Some(&s(t("insight_column"))));
        popup.setToolTip(Some(&s(t("insight_column"))));
    }
    bar.text.setAccessibilityLabel(Some(&s(match place {
        Place::Profile => t("insight_explain"),
        Place::Chart => t("insight_summarize"),
    })));
    layout(bar);
}

/// The pop-up (profile), the button and *Copy* along the top of the bar, the note after them;
/// the text under them.
fn layout(bar: &Bar) {
    let size = bar.bar.frame().size;
    let top = size.height - ROW_H;
    let mut x = 0.0;
    if let Some(popup) = &bar.column {
        popup.sizeToFit();
        let width = popup.frame().size.width.min(POPUP_MAX_W);
        popup.setFrame(NSRect::new(NSPoint::new(x, top), NSSize::new(width, ROW_H)));
        x += width + GAP;
    }
    for button in [&bar.run, &bar.copy] {
        button.sizeToFit();
        let width = button.frame().size.width;
        button.setFrame(NSRect::new(NSPoint::new(x, top), NSSize::new(width, ROW_H)));
        x += width + GAP;
    }
    bar.note.setFrame(NSRect::new(
        NSPoint::new(x, top + (ROW_H - NOTE_H) / 2.0),
        NSSize::new((size.width - x).max(0.0), NOTE_H),
    ));
    bar.scroll.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(size.width, TEXT_H),
    ));
}
