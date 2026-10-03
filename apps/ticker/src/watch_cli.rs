use crate::config::{Asset, Config};
use crate::price_input::{parse_price, parse_watch_target};
use crate::price_watch::{WatchDirection, update_watch_list};
use std::error::Error;

pub fn handle_watch_command(args: &[String]) -> Result<String, Box<dyn Error>> {
    if args.is_empty() {
        return Err("No command specified".into());
    }

    match args[0].as_str() {
        "add" => add_watch(&args[1..]),
        "remove" => remove_watch(&args[1..]),
        "list" => list_watches(),
        "clear" => clear_watches(),
        "reset" => reset_triggered(),
        "help" | "--help" | "-h" => Ok(get_help_text()),
        cmd => Err(format!("Unknown command: {}", cmd).into()),
    }
}

fn add_watch(args: &[String]) -> Result<String, Box<dyn Error>> {
    if args.len() < 3 {
        return Err("Usage: ticker add <asset_name> <target_price> <above|below>".into());
    }

    let asset_name = args[0].clone();
    let allow_negative = configured_asset(&asset_name).is_some_and(|a| a.allows_negative());
    let target_price = parse_watch_target(&args[1], allow_negative)?;
    let direction = match args[2].to_lowercase().as_str() {
        "above" => WatchDirection::Above,
        "below" => WatchDirection::Below,
        _ => return Err("Direction must be 'above' or 'below'".into()),
    };

    // Load, check, add and save under the watch-file lock: a running menu bar app cannot
    // overwrite this change, and picks it up on its next poll.
    let added = update_watch_list(|watch_list| {
        if watch_list
            .watches
            .iter()
            .any(|w| w.asset_name == asset_name && (w.target_price - target_price).abs() < 0.01)
        {
            return false;
        }
        watch_list.add_watch(asset_name.clone(), target_price, direction.clone());
        true
    })?;
    if !added {
        return Err(format!(
            "Watch already exists for {} at €{:.2}",
            asset_name, target_price
        )
        .into());
    }

    Ok(format!(
        "✅ Added watch: {} {} €{:.2}",
        direction.emoji(),
        asset_name,
        target_price
    ))
}

/// The asset `name` (case-insensitive) in the config the menu bar app uses, if it is there.
fn configured_asset(name: &str) -> Option<Asset> {
    find_asset(crate::config::load_config().ok()?, name)
}

fn find_asset(config: Config, name: &str) -> Option<Asset> {
    let wanted = name.to_lowercase();
    config
        .assets
        .into_iter()
        .find(|a| a.name.to_lowercase() == wanted)
}

fn remove_watch(args: &[String]) -> Result<String, Box<dyn Error>> {
    if args.len() < 2 {
        return Err("Usage: ticker remove <asset_name> <target_price>".into());
    }

    let asset_name = &args[0];
    let target_price = parse_price(&args[1])?;

    if update_watch_list(|watch_list| watch_list.remove_watch(asset_name, target_price))? {
        Ok(format!(
            "✅ Removed watch for {} at €{:.2}",
            asset_name, target_price
        ))
    } else {
        Err(format!(
            "❌ Watch not found for {} at €{:.2}",
            asset_name, target_price
        )
        .into())
    }
}

fn list_watches() -> Result<String, Box<dyn Error>> {
    let watch_list = update_watch_list(|watch_list| watch_list.clone())?;

    if watch_list.watches.is_empty() {
        return Ok("📭 No price watches configured".to_string());
    }

    let mut output = String::from("📋 Price Watches:\n\n");

    for (i, watch) in watch_list.watches.iter().enumerate() {
        let status = if watch.triggered { "✓" } else { " " };
        let direction_emoji = watch.direction.emoji();
        let direction_text = watch.direction.as_str();
        output.push_str(&format!(
            "{}. [{}] {} {} - €{:.2} ({})\n",
            i + 1,
            status,
            direction_emoji,
            watch.asset_name,
            watch.target_price,
            direction_text
        ));
    }

    Ok(output)
}

/// Loads first (under the lock), so a corrupt watch file is reported instead of overwritten.
fn clear_watches() -> Result<String, Box<dyn Error>> {
    let removed = update_watch_list(|watch_list| std::mem::take(&mut watch_list.watches).len())?;
    Ok(format!("✅ All price watches cleared ({removed} removed)"))
}

/// Re-arm triggered watches; reports how many were triggered (and so are re-armed now).
fn reset_triggered() -> Result<String, Box<dyn Error>> {
    let (reset, total) = update_watch_list(|watch_list| {
        let reset = watch_list.watches.iter().filter(|w| w.triggered).count();
        watch_list.reset_all_states();
        (reset, watch_list.watches.len())
    })?;

    Ok(format!(
        "✅ Re-armed {reset} triggered watch{} ({total} in total)",
        if reset == 1 { "" } else { "es" }
    ))
}

fn get_help_text() -> String {
    r#"🚀 Ticker - Price Watch Management

Usage: ticker [COMMAND] [OPTIONS]

Commands:
  add <asset> <price> <above|below>
    Add a price watch
    Example: ticker add Bitcoin 68000 above

  remove <asset> <price>
    Remove a price watch
    Example: ticker remove Bitcoin 68000

  list
    List all price watches ([✓] = triggered, waiting for a reset)
    Example: ticker list

  clear
    Clear all price watches
    Example: ticker clear

  reset
    Re-arm all triggered watches so they can fire again
    Example: ticker reset

  help, --help, -h
    Show this help message

Examples:
  # Add a watch for BTC above €68,000
  ticker add Bitcoin 68000 above

  # Add a watch for Gold below €2,000
  ticker add Gold 2000 below

  # List all watches
  ticker list

  # Remove a watch
  ticker remove Bitcoin 68000

Notes:
  - Watches are persisted in ~/.ticker_watches.json
  - When a price reaches the watch threshold, a notification will be sent
  - A watch fires once, then stays triggered until you run `ticker reset`
    (there is no automatic daily reset)
  - Prices may be written as 68000, 68.000 or 68.000,00
  - Commands may run while the menubar app runs; it picks up changes at its next poll
  - Running without arguments starts the menubar app
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static WATCH_CLI_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_watch_file<T>(f: impl FnOnce() -> T) -> T {
        let _guard = WATCH_CLI_LOCK.lock().unwrap();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ticker-cli-watches-{stamp}.json"));
        let _ = std::fs::remove_file(&path);
        // SAFETY: serialized by WATCH_CLI_LOCK for the duration of the closure.
        unsafe {
            std::env::set_var("TICKER_WATCHES_PATH", &path);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        let _ = std::fs::remove_file(&path);
        unsafe {
            std::env::remove_var("TICKER_WATCHES_PATH");
        }
        match result {
            Ok(v) => v,
            Err(e) => std::panic::resume_unwind(e),
        }
    }

    #[test]
    fn test_help_text_is_not_empty() {
        let help = get_help_text();
        assert!(!help.is_empty());
        assert!(help.contains("add"));
        assert!(help.contains("remove"));
        assert!(help.contains("list"));
    }

    #[test]
    fn help_command_aliases() {
        for cmd in ["help", "--help", "-h"] {
            let out = handle_watch_command(&[cmd.to_string()]).unwrap();
            assert!(out.contains("Usage"));
        }
    }

    #[test]
    fn unknown_command_errors() {
        let err = handle_watch_command(&["nope".to_string()]).unwrap_err();
        assert!(err.to_string().contains("Unknown command"));
    }

    #[test]
    fn empty_args_error() {
        assert!(handle_watch_command(&[]).is_err());
    }

    #[test]
    fn add_list_remove_roundtrip() {
        with_temp_watch_file(|| {
            let added = handle_watch_command(&[
                "add".into(),
                "Bitcoin".into(),
                "68000".into(),
                "above".into(),
            ])
            .unwrap();
            assert!(added.contains("Bitcoin"));

            let listed = handle_watch_command(&["list".into()]).unwrap();
            assert!(listed.contains("Bitcoin"));
            assert!(listed.contains("68000"));

            let dup = handle_watch_command(&[
                "add".into(),
                "Bitcoin".into(),
                "68000".into(),
                "above".into(),
            ]);
            assert!(dup.is_err());

            let removed =
                handle_watch_command(&["remove".into(), "Bitcoin".into(), "68000".into()]).unwrap();
            assert!(removed.contains("Removed"));

            let listed = handle_watch_command(&["list".into()]).unwrap();
            assert!(listed.contains("No price watches"));
        });
    }

    #[test]
    fn add_rejects_bad_direction() {
        with_temp_watch_file(|| {
            let err = handle_watch_command(&[
                "add".into(),
                "Gold".into(),
                "2000".into(),
                "sideways".into(),
            ])
            .unwrap_err();
            assert!(err.to_string().contains("above") || err.to_string().contains("below"));
        });
    }

    #[test]
    fn add_reads_dutch_notation_and_refuses_bad_targets() {
        with_temp_watch_file(|| {
            handle_watch_command(&[
                "add".into(),
                "Bitcoin".into(),
                "68.000".into(),
                "above".into(),
            ])
            .unwrap();
            let listed = handle_watch_command(&["list".into()]).unwrap();
            assert!(listed.contains("68000.00"), "{listed}");
            for bad in ["inf", "NaN", "0", "-1", "abc"] {
                let err = handle_watch_command(&[
                    "add".into(),
                    "Gold".into(),
                    bad.into(),
                    "below".into(),
                ]);
                assert!(err.is_err(), "{bad}");
            }
            handle_watch_command(&["remove".into(), "Bitcoin".into(), "68.000,00".into()]).unwrap();
        });
    }

    #[test]
    fn reset_counts_only_triggered_watches() {
        with_temp_watch_file(|| {
            for (asset, price) in [("Bitcoin", "1"), ("Gold", "2"), ("ETH", "3")] {
                handle_watch_command(&["add".into(), asset.into(), price.into(), "above".into()])
                    .unwrap();
            }
            // Two of the three go off.
            update_watch_list(|l| {
                l.check_price("Bitcoin", 10.0);
                l.check_price("Gold", 10.0);
            })
            .unwrap();
            let out = handle_watch_command(&["reset".into()]).unwrap();
            assert!(out.contains("Re-armed 2 triggered watches"), "{out}");
            assert!(out.contains("3 in total"), "{out}");
            let listed = handle_watch_command(&["list".into()]).unwrap();
            assert!(!listed.contains('✓'), "{listed}");
            let again = handle_watch_command(&["reset".into()]).unwrap();
            assert!(again.contains("Re-armed 0 triggered watches"), "{again}");
        });
    }

    #[test]
    fn help_text_matches_the_behaviour() {
        let help = get_help_text();
        assert!(!help.contains("once per day"));
        assert!(!help.contains("active price watches"));
        assert!(help.contains("until you run `ticker reset`"));
    }

    #[test]
    fn find_asset_is_case_insensitive_and_sees_allow_negative() {
        let bundled = || crate::config::parse_config(include_str!("../config.toml")).unwrap();
        let power = find_asset(bundled(), "power nl").expect("bundled Power NL");
        assert!(power.allows_negative());
        assert!(!find_asset(bundled(), "Gold").unwrap().allows_negative());
        assert!(find_asset(bundled(), "Platinum").is_none());
    }

    #[test]
    fn add_requires_three_args() {
        let err = handle_watch_command(&["add".into(), "Bitcoin".into()]).unwrap_err();
        assert!(err.to_string().contains("Usage"));
    }

    #[test]
    fn clear_refuses_a_corrupt_watch_file() {
        with_temp_watch_file(|| {
            let path = std::env::var("TICKER_WATCHES_PATH").unwrap();
            std::fs::write(&path, "{ not json").unwrap();
            assert!(handle_watch_command(&["clear".into()]).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
        });
    }

    #[test]
    fn clear_empties_list() {
        with_temp_watch_file(|| {
            handle_watch_command(&["add".into(), "Gold".into(), "2000".into(), "below".into()])
                .unwrap();
            let cleared = handle_watch_command(&["clear".into()]).unwrap();
            assert!(cleared.contains("cleared"));
            let listed = handle_watch_command(&["list".into()]).unwrap();
            assert!(listed.contains("No price watches"));
        });
    }
}
