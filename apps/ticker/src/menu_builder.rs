use polars::prelude::*;
use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};

use crate::price_watch::WatchList;
use crate::watch_ui::WatchUIBuilder;

const POLARS_VERSION: &str = "0.55.2";

pub struct MenuBuilder;

struct PriceRow<'a> {
    symbol: &'a str,
    name: &'a str,
    price: f64,
    unit: &'a str,
    unit_hint: &'a str,
    change: Option<f64>,
    pct: Option<f64>,
    direction: Option<&'a str>,
}

impl MenuBuilder {
    pub fn build(df: &DataFrame, watch_list: &WatchList) -> Menu {
        let menu = Menu::new();

        if df.height() == 0 {
            let _ = menu.append(&MenuItem::new("No prices yet", false, None));
        } else {
            let symbols = df.column("symbol").ok().and_then(|c| c.str().ok());
            let names = df.column("name").ok().and_then(|c| c.str().ok());
            let prices = df.column("price").ok().and_then(|c| c.f64().ok());
            let units = df.column("unit").ok().and_then(|c| c.str().ok());
            let unit_hints = df.column("unit_hint").ok().and_then(|c| c.str().ok());
            let changes = df.column("change_day").ok().and_then(|c| c.f64().ok());
            let pcts = df.column("pct_day").ok().and_then(|c| c.f64().ok());
            let directions = df.column("direction_day").ok().and_then(|c| c.str().ok());

            if let (Some(symbols), Some(names), Some(prices), Some(units), Some(unit_hints)) =
                (symbols, names, prices, units, unit_hints)
            {
                for i in 0..df.height() {
                    let name = names.get(i).unwrap_or("?");
                    let row = Self::format_price_row(&PriceRow {
                        symbol: symbols.get(i).unwrap_or(""),
                        name,
                        price: prices.get(i).unwrap_or(f64::NAN),
                        unit: units.get(i).unwrap_or(""),
                        unit_hint: unit_hints.get(i).unwrap_or(""),
                        change: changes.as_ref().and_then(|c| c.get(i)),
                        pct: pcts.as_ref().and_then(|c| c.get(i)),
                        direction: directions.as_ref().and_then(|c| c.get(i)),
                    });
                    let _ = menu.append(&MenuItem::with_id(Self::item_id(name), &row, true, None));
                }
            } else {
                let _ = menu.append(&MenuItem::new("Invalid price data", false, None));
            }
        }

        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::new(
            WatchUIBuilder::watch_status_indicator(watch_list),
            false,
            None,
        ));

        for watch in &watch_list.watches {
            let mark = if watch.triggered { "✓" } else { " " };
            let item_text = format!(
                "{} {} {}  €{}",
                mark,
                watch.direction.emoji(),
                watch.asset_name,
                Self::format_price(watch.target_price)
            );
            let item_id = format!(
                "watch_{}_{}",
                watch.asset_name.to_lowercase().replace(' ', "_"),
                watch.target_price
            );
            let _ = menu.append(&MenuItem::with_id(&item_id, &item_text, true, None));
        }

        let _ = menu.append(&MenuItem::with_id(
            "add_watch",
            "➕ Add Price Watch",
            true,
            None,
        ));
        let _ = menu.append(&MenuItem::with_id(
            "manage_watches",
            "⚙️ Manage Watches",
            true,
            None,
        ));
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::with_id("poll", "🔄  Poll now", true, None));
        let _ = menu.append(&MenuItem::with_id(
            "copy",
            "📋  Copy to clipboard",
            true,
            None,
        ));
        let _ = menu.append(&MenuItem::with_id(
            "edit_asset",
            "✏️ Edit asset…",
            true,
            None,
        ));
        let _ = menu.append(&MenuItem::with_id(
            "reset_assets",
            "↩️ Reset assets to defaults",
            true,
            None,
        ));
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&Self::version_item());
        let _ = menu.append(&MenuItem::with_id("quit", " Quit", true, None));
        menu
    }

    pub fn menubar_title(df: &DataFrame, preferred: Option<&str>) -> String {
        if df.height() == 0 {
            return "Ticker".to_string();
        }
        let names = df.column("name").ok().and_then(|c| c.str().ok());
        let prices = df.column("price").ok().and_then(|c| c.f64().ok());
        let units = df.column("unit").ok().and_then(|c| c.str().ok());
        let symbols = df.column("symbol").ok().and_then(|c| c.str().ok());
        let Some(names) = names else {
            return "Ticker".to_string();
        };
        let Some(prices) = prices else {
            return "Ticker".to_string();
        };
        let mut idx = None;
        if let Some(want) = preferred {
            for i in 0..df.height() {
                if names.get(i) == Some(want) && prices.get(i).is_some_and(|p| !p.is_nan()) {
                    idx = Some(i);
                    break;
                }
            }
        }
        if idx.is_none() {
            for i in 0..df.height() {
                if prices.get(i).is_some_and(|p| !p.is_nan()) {
                    idx = Some(i);
                    break;
                }
            }
        }
        let Some(i) = idx else {
            return "Ticker".to_string();
        };
        let price = prices.get(i).unwrap_or(f64::NAN);
        let unit = units.and_then(|c| c.get(i)).unwrap_or("EUR");
        let symbol = symbols.and_then(|c| c.get(i)).unwrap_or("");
        let currency = Self::unit_to_currency(unit);
        let price_txt = Self::format_menubar_price(price);
        if symbol.is_empty() {
            format!("{currency}{price_txt}")
        } else {
            format!("{symbol} {currency}{price_txt}")
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

    fn format_price_row(row: &PriceRow<'_>) -> String {
        let currency = Self::unit_to_currency(row.unit);
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
        let line1 = format!("{}  {}{}{}", label, currency, price_txt, unit_part);
        let line2 = match (row.change, row.pct, row.direction) {
            (Some(c), Some(p), Some("up")) => {
                format!("  ▲ {}{} · +{:.2}%", currency, Self::format_price(c), p)
            }
            (Some(c), Some(p), Some("down")) => {
                format!(
                    "  ▼ {}{} · {:.2}%",
                    currency,
                    Self::format_price(c.abs()),
                    p
                )
            }
            _ => String::new(),
        };
        if line2.is_empty() {
            line1
        } else {
            format!("{line1}\n{line2}")
        }
    }

    pub fn dataframe_as_tsv(df: &DataFrame) -> String {
        let cols = [
            "symbol",
            "name",
            "price",
            "unit",
            "unit_hint",
            "day_open",
            "change_day",
            "pct_day",
            "direction_day",
        ];
        let mut out = String::new();
        out.push_str(&cols.join("\t"));
        out.push('\n');
        if df.height() == 0 {
            return out;
        }
        for i in 0..df.height() {
            let mut cells = Vec::with_capacity(cols.len());
            for col_name in &cols {
                let cell = match df.column(col_name) {
                    Ok(col) => Self::cell_at(col, i),
                    Err(_) => String::new(),
                };
                cells.push(cell);
            }
            out.push_str(&cells.join("\t"));
            out.push('\n');
        }
        out
    }

    fn cell_at(col: &Column, row: usize) -> String {
        if let Ok(ca) = col.str()
            && let Some(s) = ca.get(row)
        {
            return s.to_string();
        }
        if let Ok(ca) = col.f64() {
            return match ca.get(row) {
                Some(v) if v.is_nan() => String::new(),
                Some(v) => format!("{:.6}", v),
                None => String::new(),
            };
        }
        String::new()
    }

    pub fn version_item() -> MenuItem {
        MenuItem::new(
            format!(
                "Version {} · Polars {}",
                env!("CARGO_PKG_VERSION"),
                POLARS_VERSION
            ),
            false,
            None,
        )
    }

    fn item_id(name: &str) -> String {
        name.to_lowercase().replace(' ', "_")
    }

    pub fn asset_name_for_item_id(df: &DataFrame, item_id: &str) -> Option<String> {
        let names = df.column("name").ok()?.str().ok()?;
        for i in 0..df.height() {
            let name = names.get(i)?;
            if Self::item_id(name) == item_id {
                return Some(name.to_string());
            }
        }
        None
    }

    fn format_price(price: f64) -> String {
        if price.is_nan() {
            return "?".to_string();
        }
        let formatted = format!("{:.2}", price);
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
            format!("{},{}", result, decimal_part)
        } else {
            formatted
        }
    }

    fn unit_to_currency(unit: &str) -> String {
        match unit {
            "EUR" => "€".to_string(),
            "USD" => "$".to_string(),
            "GBP" => "£".to_string(),
            "JPY" => "¥".to_string(),
            _ => unit.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_df() -> DataFrame {
        DataFrame::new_infer_height(vec![
            Series::new("symbol".into(), vec!["💰".to_string(), "⛽".to_string()]).into(),
            Series::new(
                "name".into(),
                vec!["Bitcoin".to_string(), "Benzine".to_string()],
            )
            .into(),
            Series::new("price".into(), vec![66553.0_f64, 2.47]).into(),
            Series::new("unit".into(), vec!["EUR".to_string(), "EUR".to_string()]).into(),
            Series::new(
                "unit_hint".into(),
                vec!["/BTC".to_string(), "/L".to_string()],
            )
            .into(),
        ])
        .expect("sample df")
    }

    #[test]
    fn version_includes_app_and_polars() {
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
        assert!(!POLARS_VERSION.is_empty());
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
    fn item_id_normalizes_name() {
        assert_eq!(MenuBuilder::item_id("TTF Gas"), "ttf_gas");
    }

    #[test]
    fn asset_name_for_item_id_roundtrip() {
        let df = sample_df();
        assert_eq!(
            MenuBuilder::asset_name_for_item_id(&df, "bitcoin").as_deref(),
            Some("Bitcoin")
        );
        assert_eq!(
            MenuBuilder::asset_name_for_item_id(&df, "benzine").as_deref(),
            Some("Benzine")
        );
        assert_eq!(MenuBuilder::asset_name_for_item_id(&df, "poll"), None);
    }

    #[test]
    fn format_price_row_two_lines_up() {
        let row = MenuBuilder::format_price_row(&PriceRow {
            symbol: "💰",
            name: "Bitcoin",
            price: 66672.0,
            unit: "EUR",
            unit_hint: "/BTC",
            change: Some(217.0),
            pct: Some(0.33),
            direction: Some("up"),
        });
        assert!(row.contains('\n'));
        let lines: Vec<_> = row.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Bitcoin"));
        assert!(lines[0].contains("66.672"));
        assert!(lines[1].contains("▲"));
        assert!(lines[1].contains("+0.33%"));
    }

    #[test]
    fn format_price_row_hides_flat_or_zero_change() {
        let row = MenuBuilder::format_price_row(&PriceRow {
            symbol: "⛽",
            name: "Benzine",
            price: 2.47,
            unit: "EUR",
            unit_hint: "/L",
            change: Some(0.0),
            pct: Some(0.0),
            direction: Some("flat"),
        });
        assert!(!row.contains('\n'));
        assert!(row.contains("Benzine"));
        assert!(row.contains("2,47"));
    }

    #[test]
    fn format_price_row_hides_missing_change() {
        let row = MenuBuilder::format_price_row(&PriceRow {
            symbol: "⚡",
            name: "Power NL",
            price: 0.21,
            unit: "EUR",
            unit_hint: "/kWh",
            change: None,
            pct: None,
            direction: None,
        });
        assert!(!row.contains('\n'));
        assert!(row.contains("Power NL"));
    }

    #[test]
    fn dataframe_as_tsv_empty_has_header() {
        let df = DataFrame::new_infer_height(vec![
            Series::new("symbol".into(), Vec::<String>::new()).into(),
            Series::new("name".into(), Vec::<String>::new()).into(),
            Series::new("price".into(), Vec::<f64>::new()).into(),
            Series::new("unit".into(), Vec::<String>::new()).into(),
            Series::new("unit_hint".into(), Vec::<String>::new()).into(),
            Series::new("day_open".into(), Vec::<Option<f64>>::new()).into(),
            Series::new("change_day".into(), Vec::<Option<f64>>::new()).into(),
            Series::new("pct_day".into(), Vec::<Option<f64>>::new()).into(),
            Series::new("direction_day".into(), Vec::<String>::new()).into(),
        ])
        .expect("empty df");
        let tsv = MenuBuilder::dataframe_as_tsv(&df);
        assert!(tsv.starts_with("symbol\tname\tprice"));
        assert_eq!(tsv.lines().count(), 1);
    }

    #[test]
    fn menubar_title_prefers_named_asset() {
        let df = sample_df();
        let title = MenuBuilder::menubar_title(&df, Some("Bitcoin"));
        assert!(title.contains("💰"));
        assert!(title.contains("€"));
        assert!(title.contains("66.553"));
    }

    #[test]
    fn menubar_title_falls_back_to_first_price() {
        let df = sample_df();
        assert!(MenuBuilder::menubar_title(&df, Some("Missing")).contains("66.553"));
    }

    #[test]
    fn menubar_title_keeps_decimals_for_small_prices() {
        let df = sample_df();
        assert!(MenuBuilder::menubar_title(&df, Some("Benzine")).contains("2,47"));
    }

    #[test]
    fn menubar_title_empty_df() {
        let df = DataFrame::new_infer_height(vec![
            Series::new("name".into(), Vec::<String>::new()).into(),
            Series::new("price".into(), Vec::<f64>::new()).into(),
        ])
        .expect("empty");
        assert_eq!(MenuBuilder::menubar_title(&df, None), "Ticker");
    }
}
