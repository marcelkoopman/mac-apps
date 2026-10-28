use super::{CodeLang, CodeOp, format_sql, looks_like_sql, minify_sql};
use crate::format::FormatKind;

#[test]
fn each_language_offers_its_steps() {
    assert_eq!(
        CodeLang::of("{\"a\":1}", FormatKind::Json),
        Some(CodeLang::Json)
    );
    assert_eq!(
        CodeLang::of("a: 1\nb:\n  - x\n", FormatKind::Yaml),
        Some(CodeLang::Yaml)
    );
    assert_eq!(
        CodeLang::of("<a><b/></a>", FormatKind::Xml),
        Some(CodeLang::Xml)
    );
    assert_eq!(
        CodeLang::of("select id, naam from klant where id = 1", FormatKind::Text),
        Some(CodeLang::Sql)
    );
    assert_eq!(CodeLang::of("hallo wereld", FormatKind::Text), None);
    assert_eq!(CodeLang::of("x = 1", FormatKind::Toml), None);
    assert_eq!(CodeLang::Json.ops(), &[CodeOp::JsonToYaml]);
    for lang in [CodeLang::Json, CodeLang::Yaml, CodeLang::Xml, CodeLang::Sql] {
        for op in lang.ops() {
            assert!(!op.title().starts_with("code_"), "{op:?}");
        }
    }
}

#[test]
fn prose_is_no_sql() {
    for prose in [
        "Select the file from the list.",
        "select a nice gift from the shop",
        "Update: the meeting is moved",
        "delete",
        "",
    ] {
        assert!(!looks_like_sql(prose), "{prose}");
    }
    for sql in [
        "SELECT * FROM orders",
        "select a, b from t",
        "insert into t (a) values (1)",
        "UPDATE t SET a = 1 WHERE id = 2",
        "delete from t where id = 3",
        "create table t (id int)",
        "with x as (select 1 from y) select * from x",
    ] {
        assert!(looks_like_sql(sql), "{sql}");
    }
}

#[test]
fn sql_formats_with_upper_case_keywords_and_a_line_per_clause() {
    let out = format_sql(
        "select o.id, o.total, c.name from orders o left join customers c on c.id = o.customer_id where o.total > 10 and c.name like 'a%' order by o.total desc limit 5",
    )
    .unwrap();
    assert_eq!(
        out,
        "SELECT\n    o.id,\n    o.total,\n    c.name\nFROM orders o\nLEFT JOIN customers c ON c.id = o.customer_id\nWHERE o.total > 10\n    AND c.name LIKE 'a%'\nORDER BY\n    o.total DESC\nLIMIT 5"
    );
}

#[test]
fn sql_keeps_strings_names_and_comments() {
    let out =
        format_sql("select \"select\", 'from where' from t -- note\nwhere a.order = 1").unwrap();
    assert!(out.contains("\"select\""), "{out}");
    assert!(out.contains("'from where'"), "{out}");
    assert!(out.contains("-- note"), "{out}");
    // A name after a dot stays as copied.
    assert!(out.contains("a.order"), "{out}");
    let out = format_sql("select count(*) from t where d between 1 and 2").unwrap();
    assert!(out.contains("COUNT(*)"), "{out}");
    assert!(out.contains("BETWEEN 1 AND 2"), "{out}");
}

#[test]
fn sql_subqueries_indent() {
    let out =
        format_sql("select a from (select a from t where b = 1) x where a in (1, 2)").unwrap();
    assert_eq!(
        out,
        "SELECT\n    a\nFROM (\n    SELECT\n        a\n    FROM t\n    WHERE b = 1\n) x\nWHERE a IN (1, 2)"
    );
}

#[test]
fn sql_statements_and_insert() {
    let out =
        format_sql("insert into t (a, b) values (1, 'x'); update t set a = 2, b = 3 where id = 1;")
            .unwrap();
    assert_eq!(
        out,
        "INSERT INTO t (a, b)\nVALUES (1, 'x');\nUPDATE t\nSET\n    a = 2,\n    b = 3\nWHERE id = 1;"
    );
}

#[test]
fn sql_minifies_to_one_line_without_comments() {
    let out = minify_sql("SELECT a,\n       b -- why\nFROM   t\n/* x */ WHERE  a >= 1;").unwrap();
    assert_eq!(out, "SELECT a, b FROM t WHERE a >= 1;");
    assert_eq!(minify_sql("hallo"), None);
}

#[test]
fn yaml_formats_minifies_and_converts() {
    let yaml = "name:   copycraft\nitems:\n  -   one\n  - two\n";
    assert_eq!(
        CodeOp::FormatYaml.apply(yaml).unwrap(),
        "name: copycraft\nitems:\n- one\n- two\n"
    );
    assert_eq!(
        CodeOp::MinifyYaml.apply(yaml).unwrap(),
        r#"{"name":"copycraft","items":["one","two"]}"#
    );
    let json = CodeOp::YamlToJson.apply(yaml).unwrap();
    assert_eq!(crate::format::detect(&json), FormatKind::Json);
    // A comment would be lost: no step.
    assert_eq!(CodeOp::FormatYaml.apply("# note\na: 1\nb: 2\n"), None);
    assert_eq!(CodeOp::MinifyYaml.apply("a: 1 # note\nb: 2\n"), None);
}

#[test]
fn json_converts_to_yaml_in_its_order() {
    let out = CodeOp::JsonToYaml
        .apply(r#"{"z": 1, "a": [true, null, "x"], "n": {"k": 1.5}}"#)
        .unwrap();
    assert_eq!(out, "z: 1\na:\n- true\n- null\n- x\nn:\n  k: 1.5\n");
    // A key twice or a number that would change: nothing.
    assert_eq!(CodeOp::JsonToYaml.apply(r#"{"a": 1, "a": 2}"#), None);
    assert_eq!(
        CodeOp::JsonToYaml.apply(r#"{"a": 123456789012345678901234}"#),
        None
    );
    assert_eq!(CodeOp::JsonToYaml.apply("{broken"), None);
}

#[test]
fn xml_formats_and_minifies() {
    let xml = "<a>\n  <b>één  twee</b>\n  <!-- c -->\n  <c x=\"1\"/>\n</a>";
    assert_eq!(
        CodeOp::MinifyXml.apply(xml).unwrap(),
        "<a><b>één twee</b><!-- c --><c x=\"1\"/></a>"
    );
    let pretty = CodeOp::FormatXml.apply("<a><b>x</b><c/></a>").unwrap();
    assert_eq!(pretty, "<a>\n    <b>x</b>\n    <c/>\n</a>");
    // CDATA keeps its exact spacing: no step.
    assert_eq!(CodeOp::MinifyXml.apply("<a><![CDATA[ x  > y ]]></a>"), None);
    assert_eq!(
        CodeOp::FormatXml.apply("<a xml:space=\"preserve\"> x </a>"),
        None
    );
}

#[test]
fn copied_sql_yaml_and_xml_get_the_code_menu() {
    for (text, lang) in [
        ("SELECT id, naam FROM klant WHERE id = 1;", CodeLang::Sql),
        ("select *\nfrom orders\nwhere total > 10", CodeLang::Sql),
        (
            "name: copycraft\nitems:\n  - one\n  - two\n",
            CodeLang::Yaml,
        ),
        ("<?xml version=\"1.0\"?>\n<a><b>x</b></a>", CodeLang::Xml),
        ("{\"a\": [1, 2]}", CodeLang::Json),
    ] {
        assert_eq!(
            CodeLang::of(text, crate::format::detect(text)),
            Some(lang),
            "{text}"
        );
    }
    let shown = crate::commands::TextShown {
        code_lang: Some(CodeLang::Yaml),
        ..Default::default()
    };
    let items: Vec<_> = crate::commands::text_menu_items(&shown)
        .into_iter()
        .map(|(cmd, _)| cmd)
        .filter(|cmd| matches!(cmd.id, crate::commands::CommandId::TextCode(_)))
        .collect();
    assert_eq!(items.len(), 3);
    for cmd in &items {
        assert_eq!(
            crate::commands::menu_group(&cmd.id),
            Some(crate::locale::t("code_menu"))
        );
        assert_eq!(
            crate::commands::text_section(&cmd.id),
            Some(crate::commands::TextSection::Convert)
        );
    }
    // A line before the conversion.
    assert!(crate::commands::separator_before(
        Some(&items[1].id),
        &items[2].id
    ));
    assert!(!crate::commands::separator_before(
        Some(&items[0].id),
        &items[1].id
    ));
    // Nothing for plain text.
    let plain = crate::commands::TextShown::default();
    assert!(
        !crate::commands::text_menu_items(&plain)
            .iter()
            .any(|(cmd, _)| matches!(cmd.id, crate::commands::CommandId::TextCode(_)))
    );
}

#[test]
fn a_code_step_runs_as_a_text_step() {
    let step = crate::text_ops::TextOp::Code(CodeOp::MinifySql);
    assert_eq!(
        step.apply("select a\nfrom t").as_deref(),
        Some("SELECT a FROM t")
    );
    assert_eq!(
        crate::text_ops::TextOp::Code(CodeOp::FormatSql).apply("hallo"),
        None
    );
}
