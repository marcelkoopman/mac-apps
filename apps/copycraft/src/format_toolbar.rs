impl FormatKind {
    pub fn shows_redact(self) -> bool {
        matches!(self, Self::Dataframe | Self::Text | Self::Plain)
    }

    pub fn shows_dataframe(self) -> bool {
        matches!(self, Self::Dataframe)
    }
}

#[cfg(test)]
mod toolbar_visibility_tests {
    use super::FormatKind;

    #[test]
    fn code_and_config_hide_redact_and_dataframe() {
        for kind in [FormatKind::Rust, FormatKind::Java, FormatKind::Yaml, FormatKind::Url] {
            assert!(!kind.shows_redact(), "{kind:?}");
            assert!(!kind.shows_dataframe(), "{kind:?}");
        }
    }

    #[test]
    fn json_hides_redact_and_dataframe() {
        assert!(!FormatKind::Json.shows_redact());
        assert!(!FormatKind::Json.shows_dataframe());
    }

    #[test]
    fn xml_hides_dataframe_tables_keep_it() {
        assert!(!FormatKind::Xml.shows_dataframe());
        assert!(!FormatKind::Xml.shows_redact());
        assert!(FormatKind::Dataframe.shows_dataframe());
        assert!(FormatKind::Dataframe.shows_redact());
    }

    #[test]
    fn prose_keeps_redact() {
        assert!(FormatKind::Text.shows_redact());
        assert!(!FormatKind::Text.shows_dataframe());
    }
}
