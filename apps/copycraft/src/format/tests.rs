use super::{FormatKind, authority_host, detect, indent_braces, looks_like_url, pretty_xml};

#[test]
fn python_is_detected_without_stealing_other_kinds() {
    assert_eq!(detect("f = open(\"demofile.txt\")"), FormatKind::Plain);
    assert_eq!(
        detect("# read it\nwith open(\"demofile.txt\") as f:\n    print(f.read())\n"),
        FormatKind::Python
    );
    assert_eq!(
        detect("import os\n\n# where am I\nprint(os.getcwd())\n"),
        FormatKind::Python
    );
    assert_eq!(
        detect("name: app\nservices:\n  web:\n    image: nginx\n"),
        FormatKind::Yaml
    );
    assert_ne!(
        detect("class Dog\n  def initialize(name)\n    @name = name\n  end\nend\n"),
        FormatKind::Python
    );
    assert_ne!(
        detect("import React from 'react';\nconst App = () => null;\n"),
        FormatKind::Python
    );
    assert_eq!(
        detect("# Notes\n\nSome text about import os and print.\n\n- item\n"),
        FormatKind::Markdown
    );
    assert_eq!(
        detect("Please import the data.\nThen print it for Jan.\n"),
        FormatKind::Text
    );
    assert_eq!(FormatKind::Python.source_heading(), "Python");
    assert_eq!(FormatKind::Python.suggested_extension(), "py");
}

#[test]
fn html_pages_are_html_not_xml() {
    for src in [
        "<!DOCTYPE html>\n<html><head><title>x</title></head><body><p>Hi</p></body></html>",
        "  \n<!doctype HTML>\n<html lang=\"nl\"><body></body></html>",
        "\u{feff}<!DocType html><html><body><br></body></html>",
        "<!DOCTYPE html><html><head><style>p { color: teal; }</style></head><body></body></html>",
        "<html>\n  <body><p>Hi</p></body>\n</html>",
        "<?xml version=\"1.0\"?>\n<!-- page -->\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><body/></html>",
    ] {
        assert_eq!(detect(src), FormatKind::Html, "{src}");
        assert_eq!(FormatKind::Html.source_heading(), "HTML");
    }
}

#[test]
fn xml_that_only_looks_like_html_stays_xml() {
    for src in [
        "<root><html>x</html></root>",
        "<htmlish><a>1</a></htmlish>",
        "<!DOCTYPE note><note><to>Jan</to></note>",
        "<!DOCTYPE htmlx><htmlx><a>1</a></htmlx>",
    ] {
        assert_eq!(detect(src), FormatKind::Xml, "{src}");
    }
}

#[test]
fn menu_symbols_are_type_marks() {
    use super::FormatKind::*;
    assert_eq!(Json.menu_symbol(), "{}");
    assert_eq!(Yaml.menu_symbol(), "---");
    assert_eq!(Rust.menu_symbol(), "fn");
    assert_eq!(Java.menu_symbol(), "Jv");
    assert_eq!(Url.menu_symbol(), "://");
    assert_eq!(Xml.menu_symbol(), "</>");
    assert_eq!(Markdown.menu_symbol(), "md");
    assert_eq!(Csv.menu_symbol(), "csv");
    assert_eq!(Tsv.menu_symbol(), "tsv");
    assert_eq!(Dataframe.menu_symbol(), "DF");
    assert_eq!(Image.menu_symbol(), "img");
    assert_eq!(Text.menu_symbol(), "¶");
    assert_eq!(Plain.menu_symbol(), "Aa");
}

#[test]
fn suggested_extensions_match_kind() {
    assert_eq!(FormatKind::Json.suggested_extension(), "json");
    assert_eq!(FormatKind::Yaml.suggested_extension(), "yaml");
    assert_eq!(FormatKind::Rust.suggested_extension(), "rs");
    assert_eq!(FormatKind::Java.suggested_extension(), "java");
    assert_eq!(FormatKind::Xml.suggested_extension(), "xml");
    assert_eq!(FormatKind::Markdown.suggested_extension(), "md");
    assert_eq!(FormatKind::Csv.suggested_extension(), "csv");
    assert_eq!(FormatKind::Tsv.suggested_extension(), "tsv");
    assert_eq!(FormatKind::Dataframe.suggested_extension(), "csv");
    assert_eq!(FormatKind::Image.suggested_extension(), "png");
    assert_eq!(FormatKind::Plain.suggested_extension(), "txt");
    assert_eq!(FormatKind::Csv.suggested_filename(), "clipboard.csv");
    assert_eq!(FormatKind::Tsv.suggested_filename(), "clipboard.tsv");
    assert_eq!(FormatKind::Dataframe.suggested_filename(), "clipboard.csv");
}

#[test]
fn formats_a_url_with_a_scheme_and_encoded_parameters() {
    assert_eq!(
        super::format_text("example.com/search?q=hello world&city=New York"),
        "https://example.com/search?q=hello%20world&city=New%20York"
    );
    assert_eq!(
        super::format_text("http://example.com/my file?x=a/b"),
        "http://example.com/my%20file?x=a%2Fb"
    );
    assert_eq!(
        super::format_text("https://example.com/search?q=hello%20world"),
        "https://example.com/search?q=hello%20world"
    );
    assert_eq!(
        super::format_text("HTTPS://example.com/path"),
        "https://example.com/path"
    );
    assert_eq!(detect("www.example.com/a"), FormatKind::Url);
    assert_eq!(detect("notes.txt"), FormatKind::Plain);
    assert_eq!(detect("https://example.com/path"), FormatKind::Url);
}

#[test]
fn authority_host_drops_userinfo() {
    assert_eq!(
        authority_host("deploy:s3cr3t@github.com"),
        Some("github.com")
    );
    assert_eq!(
        authority_host("deploy:s3cr3t@github.com:443"),
        Some("github.com")
    );
    assert_eq!(authority_host("github.com"), Some("github.com"));
    assert!(
        !authority_host("deploy:s3cr3t@github.com").is_some_and(|host| host.contains("deploy"))
    );
    assert!(looks_like_url(
        "https://deploy:s3cr3t@github.com/acme/app.git"
    ));
}

#[test]
fn detects_rust() {
    let src = "fn main() { let x = 1; }";
    assert_eq!(detect(src), FormatKind::Rust);
}

#[test]
fn detects_markdown_and_leaves_code_yaml_and_prose() {
    let notes = "# Notes\n\n- call jan\n- send the report\n";
    assert_eq!(detect(notes), FormatKind::Markdown);
    assert_eq!(detect("hello\nworld"), FormatKind::Text);
    assert_eq!(detect("# comment\necho hello"), FormatKind::Text);
    assert_eq!(
        detect("# comment\nname: copycraft\nitems:\n  - one\n"),
        FormatKind::Yaml
    );
    let fenced = "```rust\nfn main() {}\n```\n";
    assert_eq!(detect(fenced), FormatKind::Markdown);
    let readme = "# Title\n\n```rust\nfn main() {\n    let x = 1;\n}\n```\n";
    assert_eq!(detect(readme), FormatKind::Markdown);
    assert_eq!(detect("fn main() {\n    let x = 1;\n}\n"), FormatKind::Rust);
    let gitignore = "\
# =====================================================================
# 1. RUST & CARGO
# =====================================================================
/target/
debug/

# Back-up bestanden van rustfmt negeren
**/*.rs.bk
.DS_Store
!.vscode/settings.json
";
    assert_eq!(detect(gitignore), FormatKind::Text);
    assert_eq!(super::format_text(gitignore), gitignore);
}

#[test]
fn formats_markdown_spacing_and_keeps_a_tidy_document() {
    let messy = "# Title\nSome text\n- item\n- item\n## Next\npara\n";
    let formatted = super::format_text(messy);
    assert_eq!(
        formatted,
        "# Title\n\nSome text\n\n- item\n- item\n\n## Next\n\npara\n"
    );
    assert_eq!(super::format_text(&formatted), formatted);
    let tidy = "\
# AGENTS.md

## Stack

- Rust Cargo workspace, edition 2024 zoals in `Cargo.toml`. Wijzig edition niet.
- Errors: `thiserror` in libraries, `anyhow` alleen in bins/CLIs.

## Commands

Kleinste opdracht die de change dekt.
";
    assert_eq!(detect(tidy), FormatKind::Markdown);
    assert_eq!(super::format_text(tidy), tidy);
}

#[test]
fn rust_module_list_is_not_csv() {
    let src = "\
mod appearance;
mod clipboard;
mod compress;
mod convert;
mod dataframe;
mod decode;
mod format;
mod highlight;
mod icon;
mod menubar;
mod preview;
mod redact;
mod settings;
mod toolbar_visibility;
mod transform;

#[cfg(target_os = \"macos\")]
mod macos_preview_text;
#[cfg(target_os = \"macos\")]
mod macos_window;

fn main() {
    if let Err(e) = menubar::run() {
        eprintln!(\"copycraft failed: {e}\");
        std::process::exit(1);
    }
}
";
    assert_eq!(detect(src), FormatKind::Rust);
    assert!(!crate::dataframe::looks_like_csv(src));
}

#[test]
fn detects_java() {
    let src = "public class App { public static void main(String[] args) { } }";
    assert_eq!(detect(src), FormatKind::Java);
}

#[test]
fn marcel_prose_sentence_is_not_yaml() {
    let sentence = "Base64-afbeeldingen: die worden beschreven, niet getoond.";
    assert_ne!(detect(sentence), FormatKind::Yaml);
    for prose in [
        "Note: this is a test",
        "Let op: dit werkt niet.",
        "Time: 10:30",
    ] {
        assert_ne!(detect(prose), FormatKind::Yaml, "{prose}");
    }
    for yaml in [
        "version: 1.2",
        "name: copycraft",
        "key: value",
        "port: 8080",
    ] {
        assert_eq!(detect(yaml), FormatKind::Yaml, "{yaml}");
    }
}

#[test]
fn detects_yaml() {
    let src = "name: copycraft\nitems:\n  - one\n";
    assert_eq!(detect(src), FormatKind::Yaml);
}

#[test]
fn detects_xml() {
    assert_eq!(detect("<root><item/></root>"), FormatKind::Xml);
    assert_eq!(detect("<?xml version=\"1.0\"?><a></a>"), FormatKind::Xml);
}

#[test]
fn detects_csv_and_tsv() {
    assert_eq!(detect("name,age\nalice,30\nbob,40"), FormatKind::Csv);
    assert_eq!(detect("name\tage\nalice\t30\nbob\t40"), FormatKind::Tsv);
    assert_eq!(
        detect("Id;Naam;Salaris\n1;Jan;3450\n2;Anja;2900"),
        FormatKind::Csv
    );
    assert_eq!(
        detect(
            "Id,Naam,Geboortedatum,Adres,Telefoonnummer,Salaris\n\
1,Jan de Vries,1984-05-12,\"Hoofdstraat 45, Groningen\",06-12345678,3450\n\
2,Anja Bakker,1991-11-23,\"Kerkplein 2, Utrecht\",06-87654321,2900"
        ),
        FormatKind::Csv
    );
    assert_eq!(detect("just a sentence"), FormatKind::Plain);
    assert_eq!(detect("hello\nworld"), FormatKind::Text);
    assert_eq!(FormatKind::Csv.preview_heading(), "CSV");
    assert_eq!(FormatKind::Tsv.preview_heading(), "TSV");
    assert_eq!(FormatKind::Dataframe.preview_heading(), "Table");
    assert_eq!(FormatKind::Image.source_heading(), "Image");
    assert_eq!(FormatKind::Image.preview_heading(), "Image");
    assert_eq!(FormatKind::Rust.source_heading(), "Rust");
    assert_eq!(FormatKind::Rust.preview_heading(), "Formatted Rust");
    assert_eq!(FormatKind::Json.source_heading(), "JSON");
    assert_eq!(FormatKind::Java.source_heading(), "Java");
}

#[test]
fn pretty_prints_xml() {
    let out = pretty_xml("<root><item id=\"1\">hi</item><empty/></root>");
    assert!(out.contains("    <item id=\"1\">hi</item>"));
    assert!(out.contains("    <empty/>"));
    assert_eq!(out.lines().next().unwrap(), "<root>");
    assert!(!out.contains("\n        hi\n"));
}

#[test]
fn pretty_prints_xml_leaf_tags_inline() {
    let src = r#"<?xml version="1.0" encoding="UTF-8"?>
<PensioenAangifteResponse>
    <Bericht>
        <RespSrt>
            ACK
        </RespSrt>
        <IdBer>
            123456
        </IdBer>
        <LhNr>
            012345678L01
        </LhNr>
        <Empty></Empty>
    </Bericht>
    <SysteemMelding>
        Het bestand heeft geen geldige extentie.
    </SysteemMelding>
</PensioenAangifteResponse>"#;
    let out = pretty_xml(src);
    assert!(out.contains("<RespSrt>ACK</RespSrt>"));
    assert!(out.contains("<IdBer>123456</IdBer>"));
    assert!(out.contains("<LhNr>012345678L01</LhNr>"));
    assert!(out.contains("<Empty></Empty>"));
    assert!(
        out.contains("<SysteemMelding>Het bestand heeft geen geldige extentie.</SysteemMelding>")
    );
    assert!(out.contains("    <Bericht>"));
    assert!(!out.contains("\n            ACK\n"));
    assert!(!out.contains("\n            012345678L01\n"));
    assert!(out.lines().next().unwrap().starts_with("<?xml"));
}

#[test]
fn indents_braces() {
    let src = "fn main(){\nlet x=1;\n}";
    let out = indent_braces(src);
    assert!(out.contains("    let x=1;"));
    assert!(out.lines().last().unwrap().starts_with('}'));
}

#[test]
fn rust_is_formatted_in_process_by_brackets() {
    let messy = "fn a(x: u8) -> u8 {\nif x > 1 {\nlet v = f(\nx,\n);\nreturn v;\n}\nlet items = list\n.iter()\n.count();\nmatch x {\n0 => 1,\n_ => 2,\n}\n}";
    let tidy = "fn a(x: u8) -> u8 {
    if x > 1 {
        let v = f(
            x,
        );
        return v;
    }
    let items = list
        .iter()
        .count();
    match x {
        0 => 1,
        _ => 2,
    }
}";
    assert_eq!(detect(messy), FormatKind::Rust);
    assert_eq!(super::format_text(messy), tidy);
    // Already tidy code stays as it is.
    assert_eq!(super::format_text(tidy), tidy);
}

#[test]
fn brackets_in_strings_chars_and_comments_do_not_indent() {
    let src = "fn a() {\nlet s = \"{ ( [\";\nlet c = '{';\nlet q = '\\'';\nlet u = '\\u{7b}';\n// a { comment\n/* and { another */\nlet r = r#\"raw {\"#;\nfn b<'a>(x: &'a str) {}\nlet z = 1;\n}";
    let out = indent_braces(src);
    for line in out.lines().skip(1).take(9) {
        assert!(
            line.starts_with("    ") && !line.starts_with("     "),
            "{line:?}"
        );
    }
    assert_eq!(out.lines().last(), Some("}"));
}

#[test]
fn multi_line_strings_and_comments_are_kept_as_they_are() {
    let src = "fn a() {\nlet s = \"first\n  { kept\n last\";\n/*\n   { note\n*/\nlet r = r#\"\n}\"#;\nok();\n}";
    let out = indent_braces(src);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[1], "    let s = \"first");
    assert_eq!(lines[2], "  { kept");
    assert_eq!(lines[3], " last\";");
    assert_eq!(lines[5], "   { note");
    assert_eq!(lines[6], "*/");
    assert_eq!(lines[8], "}\"#;");
    assert_eq!(lines[9], "    ok();");
    assert_eq!(lines[10], "}");
}

#[test]
fn java_is_formatted_by_the_same_indenter() {
    let src = "public class A {\nprivate void f() {\nif (x) {\ny();\n}\nz();\n}\n}";
    assert_eq!(detect(src), FormatKind::Java);
    assert_eq!(
        super::format_text(src),
        "public class A {\n    private void f() {\n        if (x) {\n            y();\n        }\n        z();\n    }\n}"
    );
}

#[test]
fn blank_line_record_stays_text() {
    let src = "\
Naam: Jan de Vries

Adres: Hoofdstraat 45, 9711 AB Groningen

E-mailadres: jan.devries@email.nl

Telefoonnummer: 06-12345678

Geboortedatum: 12 mei 1984

Salaris: € 3.450";
    assert_ne!(detect(src), FormatKind::Yaml);
    let formatted = super::format_text(src);
    assert!(formatted.contains("Adres:"));
    assert!(formatted.contains("Telefoonnummer:"));
    assert!(formatted.contains("jan.devries@email.nl"));
}
