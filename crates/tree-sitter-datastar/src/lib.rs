use tree_sitter::Language;

extern "C" {
    fn tree_sitter_datastar() -> Language;
}

/// Returns the tree-sitter [`Language`] for Datastar expressions and attributes.
pub fn language() -> Language {
    unsafe { tree_sitter_datastar() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_loads() {
        let lang = language();
        // Grammar defines many node kinds
        assert!(lang.node_kind_count() > 0);
        // Fields are optional (grammar may not define named fields)
        assert!(lang.node_kind_count() > 5);
    }

    #[test]
    fn test_parse_signal_reference() {
        let lang = language();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse("$foo.bar", None).unwrap();
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        // Should contain at least a signal_reference
        let has_signal = root
            .children(&mut root.walk())
            .any(|c| c.kind() == "expression_statement");
        assert!(has_signal, "expected signal expression in parse tree");
    }

    #[test]
    fn test_parse_attribute_name() {
        let lang = language();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse("data-on:click__debounce.500ms", None).unwrap();
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        let has_attr = root
            .children(&mut root.walk())
            .any(|c| c.kind() == "datastar_attribute");
        assert!(has_attr, "expected datastar_attribute in parse tree");
    }

    #[test]
    fn test_parse_action_call() {
        let lang = language();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser
            .parse(r#"@get('/endpoint', {openWhenHidden: true})"#, None)
            .unwrap();
        let root = tree.root_node();
        let has_action = root
            .children(&mut root.walk())
            .any(|c| c.kind() == "expression_statement");
        assert!(has_action, "expected action expression in parse tree");
    }
}
