use chrono::{DateTime, Local};
use mac_ui::tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use std::collections::HashMap;

use crate::config::{Asset, SkippedAsset};
use crate::freshness::{AssetStatus, STALE_MARK};
use crate::menu_ids;
use crate::price_watch::{PriceWatch, WatchList};
use crate::prices::{Change, Direction, PriceRow};
use crate::watch_ui::WatchUIBuilder;

pub struct MenuBuilder;

/// Fetch status of every asset (by name) and the time it is judged at.
pub struct Freshness<'a> {
    pub status: &'a HashMap<String, AssetStatus>,
    pub now: DateTime<Local>,
}

impl Freshness<'_> {
    fn of(&self, name: &str) -> AssetStatus {
        self.status.get(name).copied().unwrap_or_default()
    }
}

impl MenuBuilder {
    /// `generation` goes into the row ids (see `menu_ids`); bump it for every rebuilt menu.
    pub fn build(
        rows: &[PriceRow],
        watch_list: &WatchList,
        generation: u64,
        freshness: &Freshness<'_>,
        skipped: &[SkippedAsset],
        sources: Option<&str>,
    ) -> Menu {
        let menu = Menu::new();

        if rows.is_empty() {
            let _ = menu.append(&MenuItem::new("No prices yet", false, None));
        }
        for (i, row) in rows.iter().enumerate() {
            let status = freshness.of(&row.name);
            let stale = status.is_stale(freshness.now);
            let text = Self::format_price_row(row, &status.updated_label(freshness.now), stale);
            let id = menu_ids::asset_item_id(generation, i);
            let item = MenuItem::with_id(id, &text, true, None);
            if stale {
                // Grey but still clickable (pins the asset in the menu bar).
                mac_ui::tray::set_secondary_title(&item, &text);
            }
            let _ = menu.append(&item);
        }
        for asset in skipped {
            let _ = menu.append(&mac_ui::tray::info_item(&Self::skipped_line(asset)));
        }

        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::new(
            WatchUIBuilder::watch_status_indicator(watch_list),
            false,
            None,
        ));

        for (index, watch) in watch_list.watches.iter().enumerate() {
            let item_text = Self::watch_row_text(watch, Self::unit_of(rows, &watch.asset_name));
            // A submenu, so no single click deletes a watch.
            let rearm = MenuItem::with_id(
                menu_ids::rearm_item_id(generation, index),
                "Re-arm",
                watch.triggered,
                None,
            );
            let remove = MenuItem::with_id(
                menu_ids::remove_item_id(generation, index),
                "Remove",
                true,
                None,
            );
            if let Ok(submenu) = Submenu::with_items(&item_text, true, &[&rearm, &remove]) {
                let _ = menu.append(&submenu);
            }
        }

        let _ = menu.append(&MenuItem::with_id(
            "add_watch",
            "Add Price Watch…",
            true,
            None,
        ));
        let _ = menu.append(&MenuItem::with_id(
            "manage_watches",
            "Manage Watches…",
            true,
            None,
        ));
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::with_id("poll", "Poll now", true, None));
        let _ = menu.append(&MenuItem::with_id("copy", "Copy to clipboard", true, None));
        let _ = menu.append(&MenuItem::with_id("edit_asset", "Edit asset…", true, None));
        let _ = menu.append(&MenuItem::with_id(
            "reset_assets",
            "Reset assets to defaults…",
            true,
            None,
        ));
        let _ = menu.append(&PredefinedMenuItem::separator());
        if let Some(sources) = sources {
            let _ = menu.append(&mac_ui::tray::info_item(sources));
        }
        let _ = menu.append(&Self::version_item());
        let _ = menu.append(&mac_ui::tray::quit_item("Quit"));
        menu
    }

    /// Error line for an asset that is not polled (bad URL or price path in the config).
    fn skipped_line(asset: &SkippedAsset) -> String {
        format!("{STALE_MARK} {} skipped: {}", asset.name, asset.reason)
    }

    /// Watch row (a submenu title): `✓ Bitcoin above €70.000,00` for a watch that went off, the
    /// direction as a word rather than an emoji, so VoiceOver reads it as written.
    fn watch_row_text(watch: &PriceWatch, unit: &str) -> String {
        format!(
            "{}{} {} {}",
            if watch.triggered { "✓ " } else { "" },
            watch.asset_name,
            watch.direction.as_str(),
            Self::format_money(unit, watch.target_price)
        )
    }

    /// `Data: CoinGecko, xaus.com, …`: the hosts the polled assets come from, in config order,
    /// each once (`api.` / `www.` dropped). `None` without assets.
    pub fn source_attribution(assets: &[Asset]) -> Option<String> {
        let mut names: Vec<String> = Vec::new();
        for asset in assets {
            let Some(host) = reqwest::Url::parse(&asset.url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
            else {
                continue;
            };
            let host = host
                .strip_prefix("api.")
                .or_else(|| host.strip_prefix("www."))
                .unwrap_or(&host);
            let name = if host == "coingecko.com" {
                "CoinGecko".to_string()
            } else {
                host.to_string()
            };
            if !names.contains(&name) {
                names.push(name);
            }
        }
        (!names.is_empty()).then(|| format!("Data: {}", names.join(", ")))
    }

    /// VoiceOver label of the menu bar button: the app name, the shown asset's name and price
    /// without its emoji symbol (`spoken`, see [`MenuBuilder::menubar_spoken`]) and whether a
    /// price watch went off. The red alert icon is otherwise the only sign of an alert.
    pub fn tray_accessibility_label(spoken: Option<&str>, alert: bool) -> String {
        let mut label = String::from("Price Ticker");
        if let Some(spoken) = spoken.map(str::trim).filter(|s| !s.is_empty()) {
            label.push_str(": ");
            label.push_str(spoken);
        }
        if alert {
            label.push_str(", price alert");
        }
        label
    }

    /// The asset in the menu bar: the preferred one if it has a price, else the first with one.
    fn shown_row<'a>(rows: &'a [PriceRow], preferred: Option<&str>) -> Option<&'a PriceRow> {
        preferred
            .and_then(|want| rows.iter().find(|r| r.name == want && r.has_price()))
            .or_else(|| rows.iter().find(|r| r.has_price()))
    }

    /// What VoiceOver says for the menu bar title: `Bitcoin €66.553` (name instead of the emoji
    /// symbol), with `, out of date` when stale. `None` while no asset has a price.
    pub fn menubar_spoken(
        rows: &[PriceRow],
        preferred: Option<&str>,
        freshness: &Freshness<'_>,
    ) -> Option<String> {
        let row = Self::shown_row(rows, preferred)?;
        let stale = if freshness.of(&row.name).is_stale(freshness.now) {
            ", out of date"
        } else {
            ""
        };
        Some(format!(
            "{} {}{}{stale}",
            row.name,
            Self::unit_to_currency(&row.unit),
            Self::format_menubar_price(row.price)
        ))
    }

    /// Menu bar title: the preferred asset (else the first with a price), with [`STALE_MARK`] in
    /// front when its price is stale.
    pub fn menubar_title(
        rows: &[PriceRow],
        preferred: Option<&str>,
        freshness: &Freshness<'_>,
    ) -> String {
        let Some(row) = Self::shown_row(rows, preferred) else {
            return "Ticker".to_string();
        };
        let currency = Self::unit_to_currency(&row.unit);
        let price_txt = Self::format_menubar_price(row.price);
        let mark = if freshness.of(&row.name).is_stale(freshness.now) {
            format!("{STALE_MARK} ")
        } else {
            String::new()
        };
        if row.symbol.is_empty() {
            format!("{mark}{currency}{price_txt}")
        } else {
            format!("{mark}{} {currency}{price_txt}", row.symbol)
        }
    }

    fn format_menubar_price(price: f64) -> String {
        if price.is_nan() {
            return "?".to_string();
        }
        if price.abs() >= 100.0 {
            let formatted = Self::format_price(price.round());
            formatted
                .split_once(',')
                .map(|(int, _)| int.to_string())
                .unwrap_or(formatted)
        } else {
            Self::format_price(price)
        }
    }

    /// Two lines: `symbol name  €price /unit` and `  ▲ €change · +pct% · updated` (the change
    /// part only for a day move up or down).
    fn format_price_row(row: &PriceRow, updated: &str, stale: bool) -> String {
        let currency = Self::unit_to_currency(&row.unit);
        let price_txt = Self::format_price(row.price);
        let unit_part = {
            let h = row.unit_hint.trim();
            if h.is_empty() {
                String::new()
            } else if h.starts_with('/') {
                format!(" {}", h)
            } else {
                format!(" / {}", h)
            }
        };
        let label = if row.symbol.is_empty() {
            row.name.to_string()
        } else {
            format!("{} {}", row.symbol, row.name)
        };
        let mark = if stale {
            format!("{STALE_MARK} ")
        } else {
            String::new()
        };
        let line1 = format!("{mark}{label}  {currency}{price_txt}{unit_part}");
        let change = match row.change_day {
            Some(Change {
                amount,
                pct,
                direction: Direction::Up,
            }) => format!(
                "▲ {}{} · +{:.2}% · ",
                currency,
                Self::format_price(amount),
                pct
            ),
            Some(Change {
                amount,
                pct,
                direction: Direction::Down,
            }) => format!(
                "▼ {}{} · {:.2}% · ",
                currency,
                Self::format_price(amount.abs()),
                pct
            ),
            _ => String::new(),
        };
        let mut text = format!("{line1}\n  {change}{updated}");
        let trend: Vec<String> = [
            row.sparkline.clone(),
            row.change_24h.map(|c| format!("24h {:+.2}%", c.pct)),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !trend.is_empty() {
            text.push_str("\n  ");
            text.push_str(&trend.join(" · "));
        }
        text
    }

    /// Tab-separated rows with a header (Copy to clipboard).
    pub fn prices_as_tsv(rows: &[PriceRow]) -> String {
        let mut out = String::from(
            "symbol\tname\tprice\tunit\tunit_hint\tday_open\tchange_day\tpct_day\tdirection_day\n",
        );
        let num = |v: Option<f64>| match v {
            Some(v) if !v.is_nan() => format!("{v:.6}"),
            _ => String::new(),
        };
        for row in rows {
            let cells = [
                row.symbol.clone(),
                row.name.clone(),
                num(Some(row.price)),
                row.unit.clone(),
                row.unit_hint.clone(),
                num(row.day_open),
                num(row.change_day.map(|c| c.amount)),
                num(row.change_day.map(|c| c.pct)),
                row.change_day
                    .map(|c| c.direction.as_str().to_string())
                    .unwrap_or_default(),
            ];
            out.push_str(&cells.join("\t"));
            out.push('\n');
        }
        out
    }

    /// Grey, disabled version row ([`mac_ui::tray::info_item`]).
    pub fn version_item() -> MenuItem {
        mac_ui::tray::info_item(&format!("Version {}", env!("CARGO_PKG_VERSION")))
    }

    /// Asset name in `row` of `rows` (the row index of an asset menu item).
    pub fn asset_name_at(rows: &[PriceRow], row: usize) -> Option<String> {
        rows.get(row).map(|r| r.name.clone())
    }

    /// Dutch notation with two decimals (`1.234,56`, `-0,05`); `?` for NaN.
    pub fn format_price(price: f64) -> String {
        if price.is_nan() {
            return "?".to_string();
        }
        let sign = if price < 0.0 && format!("{:.2}", price.abs()) != "0.00" {
            "-"
        } else {
            ""
        };
        let formatted = format!("{:.2}", price.abs());
        let parts: Vec<&str> = formatted.split('.').collect();
        if parts.len() == 2 {
            let integer_part = parts[0];
            let decimal_part = parts[1];
            let mut result = String::new();
            for (i, ch) in integer_part.chars().rev().enumerate() {
                if i > 0 && i % 3 == 0 {
                    result.insert(0, '.');
                }
                result.insert(0, ch);
            }
            format!("{sign}{result},{decimal_part}")
        } else {
            format!("{sign}{formatted}")
        }
    }

    /// What goes in front of a price in `unit`: the sign for EUR, USD, GBP and JPY, otherwise
    /// the code and a space (`XAU 1,00`), nothing without a unit.
    pub fn unit_to_currency(unit: &str) -> String {
        match unit.trim() {
            "EUR" => "€".to_string(),
            "USD" => "$".to_string(),
            "GBP" => "£".to_string(),
            "JPY" => "¥".to_string(),
            "" => String::new(),
            code => format!("{code} "),
        }
    }

    /// A price in `unit`, Dutch notation: `€68.000,00`, `$1,50`, `XAU 1,00`.
    pub fn format_money(unit: &str, value: f64) -> String {
        format!(
            "{}{}",
            Self::unit_to_currency(unit),
            Self::format_price(value)
        )
    }

    /// Unit of the asset `name` (case-insensitive, like watches match assets), if it is listed.
    pub fn unit_of<'a>(rows: &'a [PriceRow], name: &str) -> &'a str {
        let wanted = name.to_lowercase();
        rows.iter()
            .find(|r| r.name == name || r.name.to_lowercase() == wanted)
            .map(|r| r.unit.as_str())
            .unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(symbol: &str, name: &str, price: f64, unit_hint: &str) -> PriceRow {
        PriceRow {
            symbol: symbol.into(),
            name: name.into(),
            price,
            unit: "EUR".into(),
            unit_hint: unit_hint.into(),
            prev_price: None,
            change: None,
            day_open: None,
            change_day: None,
            change_24h: None,
            sparkline: None,
        }
    }

    fn sample() -> Vec<PriceRow> {
        vec![
            row("💰", "Bitcoin", 66553.0, "/BTC"),
            row("⛽", "Benzine", 2.47, "/L"),
        ]
    }

    fn fresh() -> HashMap<String, AssetStatus> {
        HashMap::new()
    }

    fn now() -> DateTime<Local> {
        Local::now()
    }

    fn title(rows: &[PriceRow], preferred: Option<&str>) -> String {
        MenuBuilder::menubar_title(
            rows,
            preferred,
            &Freshness {
                status: &fresh(),
                now: now(),
            },
        )
    }

    #[test]
    fn third_line_shows_sparkline_and_24h_change() {
        let mut r = row("💰", "Bitcoin", 66672.0, "/BTC");
        assert_eq!(
            MenuBuilder::format_price_row(&r, "updated 14:05", false)
                .lines()
                .count(),
            2
        );
        r.sparkline = Some("▁▃▅█".into());
        r.change_24h = Change::between(65000.0, 66672.0);
        let text = MenuBuilder::format_price_row(&r, "updated 14:05", false);
        assert_eq!(text.lines().nth(2), Some("  ▁▃▅█ · 24h +2.57%"));
        r.sparkline = None;
        let text = MenuBuilder::format_price_row(&r, "updated 14:05", false);
        assert_eq!(text.lines().nth(2), Some("  24h +2.57%"));
    }

    #[test]
    fn money_uses_the_asset_unit() {
        assert_eq!(MenuBuilder::format_money("EUR", 68000.0), "€68.000,00");
        assert_eq!(MenuBuilder::format_money("USD", 1.5), "$1,50");
        assert_eq!(MenuBuilder::format_money("XAU", 1.0), "XAU 1,00");
        assert_eq!(MenuBuilder::format_money("", 2.0), "2,00");
        let rows = sample();
        assert_eq!(MenuBuilder::unit_of(&rows, "bitcoin"), "EUR");
        assert_eq!(MenuBuilder::unit_of(&rows, "Platinum"), "");
    }

    #[test]
    fn skipped_line_names_asset_and_reason() {
        let line = MenuBuilder::skipped_line(&SkippedAsset {
            name: "Gold".into(),
            reason: "URL must start with https://".into(),
        });
        assert_eq!(
            line,
            "\u{26A0}\u{FE0E} Gold skipped: URL must start with https://"
        );
    }

    #[test]
    fn menubar_title_marks_a_stale_price() {
        let rows = sample();
        let mut status = HashMap::new();
        status.insert(
            "Bitcoin".to_string(),
            AssetStatus {
                last_ok: None,
                failed_polls: 2,
            },
        );
        let f = Freshness {
            status: &status,
            now: now(),
        };
        let title = MenuBuilder::menubar_title(&rows, Some("Bitcoin"), &f);
        assert!(title.starts_with(STALE_MARK), "{title}");
        let title = MenuBuilder::menubar_title(&rows, Some("Benzine"), &f);
        assert!(!title.contains(STALE_MARK), "{title}");
    }

    #[test]
    fn version_row_has_the_app_version() {
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }

    #[test]
    fn format_price_nan() {
        assert_eq!(MenuBuilder::format_price(f64::NAN), "?");
    }

    #[test]
    fn format_price_thousands() {
        assert_eq!(MenuBuilder::format_price(1234.56), "1.234,56");
    }

    #[test]
    fn format_price_negative() {
        assert_eq!(MenuBuilder::format_price(-0.05), "-0,05");
        assert_eq!(MenuBuilder::format_price(-123456.0), "-123.456,00");
        assert_eq!(MenuBuilder::format_price(-0.001), "0,00");
    }

    #[test]
    fn asset_name_at_row() {
        let rows = sample();
        assert_eq!(
            MenuBuilder::asset_name_at(&rows, 0).as_deref(),
            Some("Bitcoin")
        );
        assert_eq!(
            MenuBuilder::asset_name_at(&rows, 1).as_deref(),
            Some("Benzine")
        );
        assert_eq!(MenuBuilder::asset_name_at(&rows, 2), None);
    }

    #[test]
    fn format_price_row_two_lines_up() {
        let mut r = row("💰", "Bitcoin", 66672.0, "/BTC");
        r.change_day = Some(Change {
            amount: 217.0,
            pct: 0.33,
            direction: Direction::Up,
        });
        let text = MenuBuilder::format_price_row(&r, "updated 14:05", false);
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Bitcoin"));
        assert!(lines[0].contains("66.672"));
        assert!(lines[1].contains("▲"));
        assert!(lines[1].contains("+0.33%"));
        assert!(lines[1].ends_with("updated 14:05"));
        assert!(!text.contains(STALE_MARK));
    }

    #[test]
    fn format_price_row_down() {
        let mut r = row("🥇", "Gold", 3500.0, "/oz");
        r.change_day = Some(Change {
            amount: -12.5,
            pct: -0.36,
            direction: Direction::Down,
        });
        let text = MenuBuilder::format_price_row(&r, "updated 14:05", false);
        assert!(text.contains("▼ €12,50 · -0.36%"), "{text}");
    }

    #[test]
    fn format_price_row_hides_flat_or_zero_change() {
        let mut r = row("⛽", "Benzine", 2.47, "/L");
        r.change_day = Some(Change {
            amount: 0.0,
            pct: 0.0,
            direction: Direction::Flat,
        });
        let text = MenuBuilder::format_price_row(&r, "updated 14:05", false);
        let lines: Vec<_> = text.lines().collect();
        assert!(lines[0].contains("Benzine"));
        assert!(lines[0].contains("2,47"));
        assert_eq!(lines[1], "  updated 14:05");
    }

    #[test]
    fn format_price_row_marks_stale_and_hides_missing_change() {
        let r = row("⚡", "Power NL", 0.21, "/kWh");
        let text = MenuBuilder::format_price_row(&r, "updated 13:40", true);
        let lines: Vec<_> = text.lines().collect();
        assert!(lines[0].starts_with(STALE_MARK));
        assert!(lines[0].contains("Power NL"));
        assert_eq!(lines[1], "  updated 13:40");
    }

    #[test]
    fn tsv_has_header_and_one_line_per_row() {
        assert_eq!(MenuBuilder::prices_as_tsv(&[]).lines().count(), 1);
        let mut rows = sample();
        rows[1].price = f64::NAN;
        rows[0].day_open = Some(66000.0);
        rows[0].change_day = Change::between(66000.0, 66553.0);
        let tsv = MenuBuilder::prices_as_tsv(&rows);
        let lines: Vec<_> = tsv.lines().collect();
        assert!(lines[0].starts_with("symbol\tname\tprice"));
        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines[1],
            "💰\tBitcoin\t66553.000000\tEUR\t/BTC\t66000.000000\t553.000000\t0.837879\tup"
        );
        assert_eq!(lines[2], "⛽\tBenzine\t\tEUR\t/L\t\t\t\t");
    }

    #[test]
    fn tray_label_names_the_app_price_and_alert() {
        assert_eq!(
            MenuBuilder::tray_accessibility_label(None, false),
            "Price Ticker"
        );
        assert_eq!(
            MenuBuilder::tray_accessibility_label(Some(" "), false),
            "Price Ticker"
        );
        let spoken = MenuBuilder::menubar_spoken(
            &sample(),
            Some("Bitcoin"),
            &Freshness {
                status: &fresh(),
                now: now(),
            },
        );
        assert_eq!(spoken.as_deref(), Some("Bitcoin €66.553"));
        assert_eq!(
            MenuBuilder::tray_accessibility_label(spoken.as_deref(), true),
            "Price Ticker: Bitcoin €66.553, price alert"
        );
        assert_eq!(
            MenuBuilder::tray_accessibility_label(None, true),
            "Price Ticker, price alert"
        );
        let empty = MenuBuilder::menubar_spoken(
            &[],
            None,
            &Freshness {
                status: &fresh(),
                now: now(),
            },
        );
        assert_eq!(empty, None);
    }

    #[test]
    fn spoken_title_says_out_of_date_instead_of_the_mark() {
        let mut status = HashMap::new();
        let mut s = AssetStatus::default();
        s.record(false, now());
        s.record(false, now());
        status.insert("Bitcoin".to_string(), s);
        let spoken = MenuBuilder::menubar_spoken(
            &sample(),
            None,
            &Freshness {
                status: &status,
                now: now(),
            },
        )
        .unwrap();
        assert_eq!(spoken, "Bitcoin €66.553, out of date");
        assert!(!spoken.contains(STALE_MARK));
    }

    #[test]
    fn watch_rows_use_words_not_emoji() {
        let mut list = WatchList::new();
        list.add_watch(
            "Bitcoin".into(),
            70000.0,
            crate::price_watch::WatchDirection::Above,
        );
        list.add_watch(
            "Gold".into(),
            2000.0,
            crate::price_watch::WatchDirection::Below,
        );
        list.check_price("Bitcoin", 71000.0);
        assert_eq!(
            MenuBuilder::watch_row_text(&list.watches[0], "EUR"),
            "✓ Bitcoin above €70.000,00"
        );
        assert_eq!(
            MenuBuilder::watch_row_text(&list.watches[1], "USD"),
            "Gold below $2.000,00"
        );
    }

    #[test]
    fn source_attribution_lists_each_host_once() {
        let config = crate::config::parse_config(include_str!("../config.toml")).unwrap();
        assert_eq!(
            MenuBuilder::source_attribution(&config.assets).as_deref(),
            Some("Data: CoinGecko, xaus.com, eurooilwatch.com, dap.xadi.eu")
        );
        assert_eq!(MenuBuilder::source_attribution(&[]), None);
    }

    #[test]
    fn menubar_title_prefers_named_asset() {
        let t = title(&sample(), Some("Bitcoin"));
        assert!(t.contains("💰"));
        assert!(t.contains("€"));
        assert!(t.contains("66.553"));
    }

    #[test]
    fn menubar_title_falls_back_to_first_price() {
        assert!(title(&sample(), Some("Missing")).contains("66.553"));
        let mut rows = sample();
        rows[0].price = f64::NAN;
        assert!(title(&rows, Some("Bitcoin")).contains("2,47"));
    }

    #[test]
    fn menubar_title_keeps_decimals_for_small_prices() {
        assert!(title(&sample(), Some("Benzine")).contains("2,47"));
    }

    #[test]
    fn menubar_title_without_prices() {
        assert_eq!(title(&[], None), "Ticker");
    }
}
