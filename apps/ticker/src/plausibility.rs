//! Plausibility check before a fetched price may set off a watch: a glitch in a price API
//! (0, a negative number, a value off by a factor of 1000) must not fire an alert.

use crate::config::Asset;
use std::collections::HashMap;

/// What to do with a freshly fetched price.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Plausible: check the watches with it.
    Accept,
    /// Never plausible (NaN, infinite, or at or below zero for an asset without
    /// `allow_negative`): ignored for watches.
    Reject(String),
    /// A jump bigger than the asset's `max_jump_pct` from the last plausible price: it counts
    /// once the next poll confirms it (a price within `max_jump_pct` of this one).
    Unconfirmed { from: f64, pct: f64 },
}

/// Per asset (by name): the last plausible price and an unconfirmed jump, if any. Lives as long
/// as the app; the first price of an asset after start is accepted as is.
#[derive(Debug, Default)]
pub struct TriggerGate {
    reference: HashMap<String, f64>,
    pending: HashMap<String, f64>,
}

impl TriggerGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check(&mut self, asset: &Asset, price: f64) -> Verdict {
        if !price.is_finite() {
            return Verdict::Reject(format!("{price} is not a finite number"));
        }
        if price <= 0.0 && !asset.allows_negative() {
            return Verdict::Reject(format!(
                "{price} is not above 0 (set allow_negative for this asset if it can be)"
            ));
        }
        let max = asset.max_jump();
        let name = &asset.name;
        let verdict = match self.reference.get(name) {
            None => Verdict::Accept,
            Some(&from) if jump_pct(from, price) <= max => Verdict::Accept,
            Some(&from) => match self.pending.get(name) {
                Some(&jumped) if jump_pct(jumped, price) <= max => Verdict::Accept,
                _ => Verdict::Unconfirmed {
                    from,
                    pct: jump_pct(from, price),
                },
            },
        };
        match verdict {
            Verdict::Accept => {
                self.reference.insert(name.clone(), price);
                self.pending.remove(name);
            }
            Verdict::Unconfirmed { .. } => {
                self.pending.insert(name.clone(), price);
            }
            Verdict::Reject(_) => {}
        }
        verdict
    }

    /// Forget assets that are no longer in the config (edited or reset).
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.reference.retain(|name, _| keep(name));
        self.pending.retain(|name, _| keep(name));
    }
}

/// Move from `from` to `to` in percent of `|from|`; infinite from exactly zero (unless `to` is
/// zero too).
fn jump_pct(from: f64, to: f64) -> f64 {
    let diff = (to - from).abs();
    if diff == 0.0 {
        0.0
    } else if from == 0.0 {
        f64::INFINITY
    } else {
        diff / from.abs() * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str, allow_negative: Option<bool>, max_jump_pct: Option<f64>) -> Asset {
        Asset {
            name: name.into(),
            url: "https://example.com".into(),
            price_path: "p".into(),
            unit: "EUR".into(),
            unit_hint: String::new(),
            symbol: String::new(),
            allow_negative,
            max_jump_pct,
        }
    }

    fn accepts(gate: &mut TriggerGate, a: &Asset, price: f64) -> bool {
        gate.check(a, price) == Verdict::Accept
    }

    #[test]
    fn rejects_nan_inf_and_non_positive_without_allow_negative() {
        let mut gate = TriggerGate::new();
        let btc = asset("BTC", None, None);
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -1.0] {
            assert!(matches!(gate.check(&btc, bad), Verdict::Reject(_)), "{bad}");
        }
        // A rejected price does not become the reference.
        assert!(accepts(&mut gate, &btc, 100.0));
        assert!(matches!(gate.check(&btc, 0.0), Verdict::Reject(_)));
        assert!(accepts(&mut gate, &btc, 101.0));
    }

    #[test]
    fn allow_negative_accepts_zero_and_negative_but_not_nan() {
        let mut gate = TriggerGate::new();
        let power = asset("Power", Some(true), Some(300.0));
        assert!(accepts(&mut gate, &power, -0.01));
        assert!(accepts(&mut gate, &power, -0.02));
        assert!(matches!(gate.check(&power, f64::NAN), Verdict::Reject(_)));
    }

    #[test]
    fn a_big_jump_needs_the_next_poll_to_confirm_it() {
        let mut gate = TriggerGate::new();
        let btc = asset("BTC", None, None);
        assert!(accepts(&mut gate, &btc, 100.0));
        assert!(accepts(&mut gate, &btc, 125.0), "exactly 25% is fine");
        assert_eq!(
            gate.check(&btc, 250.0),
            Verdict::Unconfirmed {
                from: 125.0,
                pct: 100.0
            }
        );
        assert!(
            accepts(&mut gate, &btc, 252.0),
            "confirmed by the next poll"
        );
        assert!(accepts(&mut gate, &btc, 255.0), "new reference");
    }

    #[test]
    fn a_one_poll_glitch_never_counts_and_does_not_delay_the_real_price() {
        let mut gate = TriggerGate::new();
        let gold = asset("Gold", None, None);
        assert!(accepts(&mut gate, &gold, 2500.0));
        assert!(matches!(
            gate.check(&gold, 2_500_000.0),
            Verdict::Unconfirmed { .. }
        ));
        assert!(accepts(&mut gate, &gold, 2501.0));
        // Two different glitches in a row do not confirm each other.
        assert!(matches!(
            gate.check(&gold, 25.0),
            Verdict::Unconfirmed { .. }
        ));
        assert!(matches!(
            gate.check(&gold, 25_000.0),
            Verdict::Unconfirmed { .. }
        ));
        assert!(accepts(&mut gate, &gold, 2499.0));
    }

    #[test]
    fn per_asset_threshold_and_zero_reference() {
        let mut gate = TriggerGate::new();
        let power = asset("Power", Some(true), Some(300.0));
        assert!(accepts(&mut gate, &power, 0.25));
        assert!(accepts(&mut gate, &power, 1.0), "+300%");
        assert!(matches!(
            gate.check(&power, 5.0),
            Verdict::Unconfirmed { .. }
        ));
        assert!(accepts(&mut gate, &power, 5.0));
        let mut gate = TriggerGate::new();
        assert!(accepts(&mut gate, &power, 0.0));
        assert!(accepts(&mut gate, &power, 0.0));
        assert!(matches!(
            gate.check(&power, 0.05),
            Verdict::Unconfirmed { .. }
        ));
        assert!(accepts(&mut gate, &power, 0.05));
    }

    #[test]
    fn retain_forgets_removed_assets() {
        let mut gate = TriggerGate::new();
        let a = asset("A", None, None);
        assert!(accepts(&mut gate, &a, 100.0));
        gate.retain(|name| name != "A");
        assert!(accepts(&mut gate, &a, 1000.0), "first price again");
    }
}
