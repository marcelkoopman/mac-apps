//! One poll's prices: a row per asset with its change since the last poll and since the day's
//! first price. A plain struct (this replaced a Polars DataFrame of seven rows).

use std::collections::HashMap;

use crate::config::Asset;

/// Moves smaller than this (in percent of the earlier price) count as [`Direction::Flat`].
pub const FLAT_PCT: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Flat,
}

impl Direction {
    /// Up / down for a move of at least [`FLAT_PCT`] percent either way, else flat. Relative,
    /// so a cent on €0,20/kWh power counts and a cent on €66.000 bitcoin does not.
    pub fn of_pct(pct: f64) -> Direction {
        if pct >= FLAT_PCT {
            Direction::Up
        } else if pct <= -FLAT_PCT {
            Direction::Down
        } else {
            Direction::Flat
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Up => "up",
            Direction::Down => "down",
            Direction::Flat => "flat",
        }
    }
}

/// Change from `base` to a later price.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change {
    pub amount: f64,
    /// Percent of `|base|` (so a rise from a negative price is positive).
    pub pct: f64,
    pub direction: Direction,
}

impl Change {
    /// `None` when either price is missing (NaN) or `base` is 0.
    pub fn between(base: f64, price: f64) -> Option<Change> {
        if price.is_nan() || base.is_nan() || base == 0.0 {
            return None;
        }
        let amount = price - base;
        let pct = amount / base.abs() * 100.0;
        Some(Change {
            amount,
            pct,
            direction: Direction::of_pct(pct),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriceRow {
    pub symbol: String,
    pub name: String,
    /// NaN when the poll got no price (and none was kept from the last poll).
    pub price: f64,
    pub unit: String,
    pub unit_hint: String,
    /// Price of the asset in the previous poll.
    pub prev_price: Option<f64>,
    /// Since the previous poll.
    pub change: Option<Change>,
    /// First price of the local calendar day (or this price when there is none yet).
    pub day_open: Option<f64>,
    /// Since `day_open`.
    pub change_day: Option<Change>,
}

impl PriceRow {
    pub fn new(asset: &Asset, price: f64) -> PriceRow {
        PriceRow {
            symbol: asset.symbol.clone(),
            name: asset.name.clone(),
            price,
            unit: asset.unit.clone(),
            unit_hint: asset.unit_hint.clone(),
            prev_price: None,
            change: None,
            day_open: None,
            change_day: None,
        }
    }

    pub fn has_price(&self) -> bool {
        !self.price.is_nan()
    }
}

/// Name → price of every row with a price.
pub fn prices_by_name(rows: &[PriceRow]) -> HashMap<String, f64> {
    rows.iter()
        .filter(|r| r.has_price())
        .map(|r| (r.name.clone(), r.price))
        .collect()
}

/// Current price of `asset` (exact name), if it has one.
pub fn price_of(rows: &[PriceRow], asset: &str) -> Option<f64> {
    rows.iter()
        .find(|r| r.name == asset && r.has_price())
        .map(|r| r.price)
}

/// Fill in the change since `prev` (name → price of the previous poll, `None` on the first poll)
/// and since the day's first price (`day_opens`, name → price; an asset without one opens at
/// its current price).
pub fn attach_changes(
    rows: &mut [PriceRow],
    prev: Option<&HashMap<String, f64>>,
    day_opens: &HashMap<String, f64>,
) {
    for row in rows {
        row.prev_price = prev.and_then(|m| m.get(&row.name).copied());
        row.change = row.prev_price.and_then(|p| Change::between(p, row.price));
        row.day_open = day_opens
            .get(&row.name)
            .copied()
            .filter(|o| !o.is_nan() && *o != 0.0)
            .or_else(|| (row.has_price() && row.price != 0.0).then_some(row.price));
        row.change_day = row.day_open.and_then(|o| Change::between(o, row.price));
    }
}

/// A row without a price this poll keeps the price of the same asset in `prev`.
pub fn fill_nan_from_prev(rows: &mut [PriceRow], prev: &[PriceRow]) {
    let kept = prices_by_name(prev);
    for row in rows.iter_mut().filter(|r| !r.has_price()) {
        if let Some(p) = kept.get(&row.name) {
            row.price = *p;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, price: f64) -> PriceRow {
        PriceRow {
            symbol: "X".into(),
            name: name.into(),
            price,
            unit: "EUR".into(),
            unit_hint: "/u".into(),
            prev_price: None,
            change: None,
            day_open: None,
            change_day: None,
        }
    }

    #[test]
    fn direction_thresholds() {
        assert_eq!(Direction::of_pct(0.0), Direction::Flat);
        assert_eq!(Direction::of_pct(0.049), Direction::Flat);
        assert_eq!(Direction::of_pct(-0.049), Direction::Flat);
        assert_eq!(Direction::of_pct(0.05), Direction::Up);
        assert_eq!(Direction::of_pct(-0.05), Direction::Down);
    }

    #[test]
    fn poll_and_day_up() {
        let mut rows = vec![row("Bitcoin", 110.0)];
        let prev = HashMap::from([("Bitcoin".to_string(), 100.0)]);
        let opens = HashMap::from([("Bitcoin".to_string(), 100.0)]);
        attach_changes(&mut rows, Some(&prev), &opens);
        let change = rows[0].change.unwrap();
        assert_eq!(change.amount, 10.0);
        assert!((change.pct - 10.0).abs() < 1e-9);
        assert_eq!(change.direction, Direction::Up);
        assert_eq!(rows[0].change_day.unwrap().direction, Direction::Up);
    }

    #[test]
    fn day_down_poll_flat() {
        let mut rows = vec![row("Gold", 100.005)];
        let prev = HashMap::from([("Gold".to_string(), 100.0)]);
        let opens = HashMap::from([("Gold".to_string(), 110.0)]);
        attach_changes(&mut rows, Some(&prev), &opens);
        assert_eq!(rows[0].change.unwrap().direction, Direction::Flat);
        assert_eq!(rows[0].change_day.unwrap().direction, Direction::Down);
    }

    #[test]
    fn nan_price_has_no_change() {
        let mut rows = vec![row("Gas", f64::NAN)];
        let prev = HashMap::from([("Gas".to_string(), 40.0)]);
        let opens = HashMap::from([("Gas".to_string(), 40.0)]);
        attach_changes(&mut rows, Some(&prev), &opens);
        assert_eq!(rows[0].change, None);
        assert_eq!(rows[0].change_day, None);
        assert_eq!(rows[0].prev_price, Some(40.0));
    }

    #[test]
    fn missing_open_uses_current_price() {
        let mut rows = vec![row("Power", 50.0)];
        attach_changes(&mut rows, None, &HashMap::new());
        assert_eq!(rows[0].day_open, Some(50.0));
        assert_eq!(rows[0].change_day.unwrap().direction, Direction::Flat);
        assert_eq!(rows[0].change, None);
    }

    #[test]
    fn small_prices_move_and_big_prices_stay_flat_on_the_same_cent() {
        let mut rows = vec![row("Power NL", 0.21), row("Bitcoin", 66010.0)];
        let opens = HashMap::from([
            ("Power NL".to_string(), 0.20),
            ("Bitcoin".to_string(), 66000.0),
        ]);
        attach_changes(&mut rows, None, &opens);
        assert_eq!(rows[0].change_day.unwrap().direction, Direction::Up);
        assert_eq!(rows[1].change_day.unwrap().direction, Direction::Flat);
    }

    #[test]
    fn pct_change_of_negative_price_keeps_the_sign_of_the_move() {
        let change = Change::between(-0.05, 0.05).unwrap();
        assert!((change.pct - 200.0).abs() < 1e-9);
        assert_eq!(change.direction, Direction::Up);
        assert_eq!(Change::between(0.0, 1.0), None);
    }

    #[test]
    fn fill_nan_keeps_the_last_price() {
        let prev = vec![row("Gold", 2000.0), row("Gas", f64::NAN)];
        let mut rows = vec![row("Gold", f64::NAN), row("Gas", f64::NAN), row("New", 1.0)];
        fill_nan_from_prev(&mut rows, &prev);
        assert_eq!(rows[0].price, 2000.0);
        assert!(rows[1].price.is_nan());
        assert_eq!(rows[2].price, 1.0);
        assert_eq!(price_of(&rows, "Gold"), Some(2000.0));
        assert_eq!(price_of(&rows, "Gas"), None);
        assert_eq!(prices_by_name(&rows).len(), 2);
    }
}
