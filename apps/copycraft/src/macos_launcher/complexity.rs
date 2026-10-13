//! Lines… › Complexity: score the code on the card off the main thread, pop the per-function
//! list under Lines…, and scroll the card to a picked function.

use super::*;
use crate::complexity::{Band, Lang};
use mac_ui::objc2_app_kit::{NSColor, NSFont};

/// Score the text the card shows (the lines the rows refer to) and pop the list.
pub fn show_complexity() {
    let painted = PAINTED.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|p| (zeroize::Zeroizing::new(p.text.clone()), p.highlight))
    });
    let Some((text, highlight)) = painted else {
        return;
    };
    let lang = highlight
        .and_then(Lang::of)
        .or_else(|| Lang::of(crate::format::detect(&text)));
    let Some(lang) = lang else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("copycraft-complexity".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let (rows, footer) = match crate::complexity::analyze_guarded(&text, lang) {
                Ok(scores) => {
                    crate::diag::log(crate::diag::LogEvent::ComplexityScored {
                        functions: scores.len(),
                        ms: started.elapsed().as_millis() as u64,
                    });
                    (
                        commands::complexity_rows(&scores),
                        commands::complexity_footer(&scores),
                    )
                }
                Err(refused) => (Vec::new(), vec![commands::complexity_refused(refused)]),
            };
            mac_ui::actions::run_on_main(move || pop_complexity(rows, footer));
        });
    if let Err(e) = spawned {
        crate::diag::log(crate::diag::LogEvent::WorkNotStarted(
            crate::diag::Work::Complexity,
            crate::diag::ErrorKind::io(&e),
        ));
    }
}

/// Lines… › Outline (`check` false) or Check on agent instructions: worked out off the main
/// thread from the text the card shows, then popped like Complexity.
pub fn show_agent_lists(check: bool) {
    let painted = PAINTED.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|p| zeroize::Zeroizing::new(p.text.clone()))
    });
    let Some(text) = painted else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("copycraft-agent".into())
        .spawn(move || {
            let (rows, footer) = if check {
                let warnings = crate::agent::check(&text);
                crate::diag::log(crate::diag::LogEvent::AgentChecked {
                    warnings: warnings.len(),
                    secrets: warnings
                        .iter()
                        .filter(|w| matches!(w, crate::agent::Warning::Secret { .. }))
                        .count(),
                });
                let empty = warnings.is_empty();
                let rows = commands::agent_check_rows(&warnings);
                let footer = if empty {
                    vec![crate::locale::t("agent_no_warnings").to_string()]
                } else {
                    Vec::new()
                };
                (rows, footer)
            } else {
                let headings = crate::agent::headings(&text);
                crate::diag::log(crate::diag::LogEvent::AgentOutline {
                    headings: headings.len(),
                });
                let footer = if headings.is_empty() {
                    vec![crate::locale::t("agent_no_headings").to_string()]
                } else {
                    Vec::new()
                };
                (commands::agent_outline_rows(&headings), footer)
            };
            mac_ui::actions::run_on_main(move || pop_complexity(rows, footer));
        });
    if let Err(e) = spawned {
        crate::diag::log(crate::diag::LogEvent::WorkNotStarted(
            crate::diag::Work::Agent,
            crate::diag::ErrorKind::io(&e),
        ));
    }
}

/// The view the Lines… chip is, else the search field.
fn lines_anchor() -> Option<Retained<NSView>> {
    let index = SHOWN.with(|slot| {
        slot.borrow()
            .iter()
            .position(|cmd| cmd.id == CommandId::TextMenu)
    });
    let chip = index.and_then(|index| CHIPS.with(|slot| slot.borrow().get(index).cloned()));
    match chip {
        Some(chip) => Some(chip.view().retain()),
        None => FIELD.with(|slot| {
            slot.borrow()
                .as_ref()
                .map(|field| Retained::into_super(Retained::into_super(field.clone())))
        }),
    }
}

fn pop_complexity(rows: Vec<(Command, Band)>, footer: Vec<String>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(anchor) = lines_anchor() else {
        return;
    };
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(""));
    menu.setAutoenablesItems(false);
    let font = NSFont::menuFontOfSize(0.0);
    for (index, (cmd, band)) in rows.iter().enumerate() {
        let item = mac_ui::actions::menu_item_with_action(mtm, &cmd.title, sel!(poppedClicked:));
        item.setTag(index as isize);
        item.setEnabled(cmd.enabled);
        let color = match band {
            Band::Normal => NSColor::labelColor(),
            Band::High => NSColor::systemOrangeColor(),
            Band::VeryHigh => NSColor::systemRedColor(),
        };
        let title = mac_ui::text::AttrText::new(&cmd.title, &font, &color).into_attributed();
        item.setAttributedTitle(Some(&title));
        item.setToolTip(Some(&NSString::from_str(&cmd.detail)));
        DELEGATE.with(|slot| {
            if let Some(delegate) = slot.borrow().as_ref() {
                mac_ui::actions::set_menu_target(&item, delegate);
            }
        });
        menu.addItem(&item);
    }
    if !rows.is_empty() && !footer.is_empty() {
        menu.addItem(&NSMenuItem::separatorItem(mtm));
    }
    for line in &footer {
        let item = mac_ui::widgets::menu_item(mtm, line);
        item.setEnabled(false);
        menu.addItem(&item);
    }
    POPPED.with(|slot| set_commands(slot, rows.into_iter().map(|(cmd, _)| cmd).collect()));
    let below = NSPoint::new(0.0, anchor.frame().size.height + 4.0);
    menu.popUpMenuPositioningItem_atLocation_inView(None, below, Some(&anchor));
    focus_card();
}

/// Scroll the card's text to `line` (1-based) and flash it.
pub fn scroll_card_to_line(line: usize) {
    let offset = PAINTED.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|p| crate::complexity::line_offset_utf16(&p.text, line))
    });
    let Some(offset) = offset else {
        return;
    };
    PREVIEW_TEXT.with(|slot| {
        let borrowed = slot.borrow();
        let Some(view) = borrowed.as_ref() else {
            return;
        };
        let range = NSRange {
            location: offset,
            length: 0,
        };
        view.scrollRangeToVisible(range);
        // The line's first character, so the find indicator has something to show.
        let length = view.string().length().saturating_sub(offset).min(1);
        view.showFindIndicatorForRange(NSRange {
            location: offset,
            length,
        });
    });
}
