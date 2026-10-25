//! The privacy manifest (PrivacyInfo.xcprivacy) and the export-compliance key are part of what
//! the App Store checks. Plain string checks, so this runs on Linux too; the release workflow and
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
            "NSPrivacyAccessedAPICategoryUserDefaults",
            "NSPrivacyAccessedAPICategoryFileTimestamp",
            "NSPrivacyAccessedAPICategorySystemBootTime",
        ]
    );
    for reason in ["CA92.1", "C617.1", "3B52.1", "35F9.1"] {
        assert!(
            xml.contains(&format!("<string>{reason}</string>")),
            "{reason}"
        );
    }
}

#[test]
fn bundle_config_ships_the_manifest_and_the_export_key() {
    assert!(read("Cargo.toml").contains("resources = [\"PrivacyInfo.xcprivacy\"]"));
    let ext = read("assets/Info.plist.ext");
    assert!(ext.contains("<key>ITSAppUsesNonExemptEncryption</key>\n  <false/>"));
}

/// The Screen Recording reason macOS shows for *Take Screenshot…*: in the Info.plist (English)
/// and in en/nl InfoPlist.strings (scripts/finish_info_plist.sh copies them into the bundle).
#[test]
fn screen_recording_reason_is_there_in_both_languages() {
    let ext = read("assets/Info.plist.ext");
    let plist = strings_after(&ext, "NSScreenCaptureUsageDescription");
    assert_eq!(plist.len(), 1);
    let reason = |lang: &str| {
        let strings = read(&format!("assets/{lang}.lproj/InfoPlist.strings"));
        let line = strings
            .lines()
            .find_map(|l| l.strip_prefix("\"NSScreenCaptureUsageDescription\" = \""))
            .unwrap_or_else(|| panic!("{lang}: no NSScreenCaptureUsageDescription"));
        line.strip_suffix("\";").expect("ends with \";").to_string()
    };
    let (en, nl) = (reason("en"), reason("nl"));
    assert_eq!(en, plist[0]);
    assert_ne!(en, nl);
    for text in [&en, &nl] {
        assert!(!text.contains('"') && text.contains("CopyCraft"));
    }
    let script = read("../../scripts/finish_info_plist.sh");
    assert!(
        script.contains("InfoPlist.strings") && script.contains("NSScreenCaptureUsageDescription")
    );
}
