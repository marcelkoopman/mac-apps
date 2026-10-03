//! Menu item ids for the dynamic rows (assets and watches).
//!
//! Ids carry the row index plus the generation of the menu they were built for, never the asset
//! name: names with `_`, spaces, non-ASCII letters or names equal to a fixed id ("poll",
//! "quit", ...) cannot collide or fail to round-trip. A click on a row of an outdated menu
//! (generation mismatch) is ignored instead of hitting the wrong row.

/// Fixed ids ("poll", "add_watch", ...) never contain this separator.
const SEP: char = '#';
const ASSET: &str = "asset";
const REARM: &str = "rearm";
const REMOVE: &str = "remove";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowRef {
    /// Row of the price rows.
    Asset(usize),
    /// *Re-arm* in the submenu of the watch at this index into `WatchList::watches`.
    Rearm(usize),
    /// *Remove* in the submenu of that watch.
    Remove(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parsed {
    /// Not a row id (a fixed id such as "poll").
    Fixed,
    /// Row of the menu with this generation.
    Row { generation: u64, row: RowRef },
}

pub fn asset_item_id(generation: u64, row: usize) -> String {
    format!("{ASSET}{SEP}{generation}{SEP}{row}")
}

pub fn rearm_item_id(generation: u64, index: usize) -> String {
    format!("{REARM}{SEP}{generation}{SEP}{index}")
}

pub fn remove_item_id(generation: u64, index: usize) -> String {
    format!("{REMOVE}{SEP}{generation}{SEP}{index}")
}

pub fn parse(id: &str) -> Parsed {
    let mut parts = id.split(SEP);
    let (Some(kind), Some(generation), Some(index), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Parsed::Fixed;
    };
    let (Ok(generation), Ok(index)) = (generation.parse(), index.parse()) else {
        return Parsed::Fixed;
    };
    let row = match kind {
        ASSET => RowRef::Asset(index),
        REARM => RowRef::Rearm(index),
        REMOVE => RowRef::Remove(index),
        _ => return Parsed::Fixed,
    };
    Parsed::Row { generation, row }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXED: &[&str] = &[
        mac_ui::tray::QUIT_ID,
        "poll",
        "copy",
        "add_watch",
        "manage_watches",
        "edit_asset",
        "reset_assets",
    ];

    #[test]
    fn fixed_ids_are_not_rows() {
        for id in FIXED {
            assert_eq!(parse(id), Parsed::Fixed, "{id}");
        }
    }

    #[test]
    fn row_ids_round_trip_and_never_equal_fixed_ids() {
        for generation in [0, 1, u64::MAX] {
            for i in [0, 1, 7, 10_000] {
                let ids = [
                    (asset_item_id(generation, i), RowRef::Asset(i)),
                    (rearm_item_id(generation, i), RowRef::Rearm(i)),
                    (remove_item_id(generation, i), RowRef::Remove(i)),
                ];
                for (id, row) in &ids {
                    assert!(!FIXED.contains(&id.as_str()), "{id}");
                    assert_eq!(
                        parse(id),
                        Parsed::Row {
                            generation,
                            row: *row
                        }
                    );
                }
                assert_ne!(ids[0].0, ids[1].0);
                assert_ne!(ids[1].0, ids[2].0);
            }
        }
    }

    #[test]
    fn malformed_ids_are_fixed() {
        for id in [
            "",
            "#",
            "asset#",
            "asset#1",
            "asset#x#1",
            "asset#1#-1",
            "asset#1#2#3",
            "other#1#2",
            "watch_ttf_gas_30",
            "watch#1#2",
            "ölpreis",
        ] {
            assert_eq!(parse(id), Parsed::Fixed, "{id}");
        }
    }
}
