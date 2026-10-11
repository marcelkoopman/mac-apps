//! The privacy manifest (PrivacyInfo.xcprivacy) is part of what the App Store checks. Plain
//! string checks, so this runs on Linux too; the release workflow and
//! scripts/check_privacy_manifest.sh run `plutil -lint` on the built bundle.

use std::path::PathBuf;

fn read(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The `<string>` values that follow each `<key>name</key>`, in file order.
fn strings_after(xml: &str, name: &str) -> Vec<String> {
    let key = format!("<key>{name}</key>");
    let mut out = Vec::new();
    let mut lines = xml.lines().map(str::trim);
    while let Some(line) = lines.next() {
        if line == key
            && let Some(value) = lines.next().and_then(|l| l.strip_prefix("<string>"))
        {
            out.push(value.trim_end_matches("</string>").to_string());
        }
    }
    out
}

#[test]
fn manifest_is_a_plist_with_the_expected_keys() {
    let xml = read("PrivacyInfo.xcprivacy");
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(xml.contains("<plist version=\"1.0\">") && xml.trim_end().ends_with("</plist>"));
    for tag in ["dict", "array"] {
        assert_eq!(
            xml.matches(&format!("<{tag}>")).count(),
            xml.matches(&format!("</{tag}>")).count(),
            "unbalanced <{tag}>"
        );
    }
    assert!(xml.contains("<key>NSPrivacyTracking</key>\n\t<false/>"));
    assert!(xml.contains("<key>NSPrivacyTrackingDomains</key>\n\t<array/>"));
    assert!(xml.contains("<key>NSPrivacyCollectedDataTypes</key>\n\t<array/>"));
    assert_eq!(
        strings_after(&xml, "NSPrivacyAccessedAPIType"),
        [
            "NSPrivacyAccessedAPICategoryFileTimestamp",
            "NSPrivacyAccessedAPICategorySystemBootTime",
        ]
    );
    for reason in ["C617.1", "35F9.1"] {
        assert!(
            xml.contains(&format!("<string>{reason}</string>")),
            "{reason}"
        );
    }
}

#[test]
fn bundle_config_ships_the_manifest() {
    assert!(read("Cargo.toml").contains("\"PrivacyInfo.xcprivacy\""));
}
