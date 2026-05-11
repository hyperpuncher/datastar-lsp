use tree_sitter::Node;

/// The position of the cursor relative to a Datastar attribute.
#[derive(Debug, Clone)]
pub enum CursorPosition {
    /// Cursor is inside a `data-*` attribute name, before any colon.
    /// e.g. `data-on|:click`, `data-te|xt`
    AttributeName { plugin_name: String },
    /// Cursor is after the colon in a `data-on:...` — modifiers context.
    AfterColon {
        plugin_name: String,
        key: Option<String>,
    },
    /// Cursor is inside a `data-*` attribute value.
    AttributeValue {
        plugin_name: String,
        value_start: usize,
        full_value: String,
    },
    /// Cursor is inside HTML/JSX markup but not in a data-* attribute — offer attribute completions.
    InMarkup,
    /// Cursor is not in any Datastar-related position.
    None,
}

/// Use tree-sitter node walking to determine the cursor position context.
pub fn detect(root: Node, source: &str, offset: usize) -> CursorPosition {
    // Find the attribute node containing the cursor
    let Some(attr_node) = find_attr_node(root, offset) else {
        return if is_in_markup(root, offset) {
            CursorPosition::InMarkup
        } else {
            CursorPosition::None
        };
    };

    let Some(name_node) = attr_name_child(attr_node) else {
        return CursorPosition::InMarkup;
    };

    let Ok(name_text) = name_node.utf8_text(source.as_bytes()) else {
        return CursorPosition::None;
    };

    if !name_text.starts_with("data-") {
        return CursorPosition::InMarkup;
    }

    let name_start = name_node.start_byte();

    // Check if cursor is in the attribute name
    if offset >= name_start && offset <= name_node.end_byte() {
        if let Some(colon_pos) = name_text.find(':') {
            if offset > name_start + colon_pos {
                return after_colon(name_text);
            }
        }
        return attribute_name(name_text);
    }

    // Check if cursor is in the attribute value
    if let Some(val) = find_value_in_attr(attr_node, source, offset) {
        return val;
    }

    CursorPosition::InMarkup
}

// ── Helpers ──

fn plugin_name(name: &str) -> &str {
    name.strip_prefix("data-")
        .unwrap_or(name)
        .split(':')
        .next()
        .unwrap_or(name)
}

fn attribute_name(name_text: &str) -> CursorPosition {
    CursorPosition::AttributeName {
        plugin_name: plugin_name(name_text).to_string(),
    }
}

fn after_colon(name_text: &str) -> CursorPosition {
    let after = &name_text[5..]; // strip "data-"
    let (p_name, key) = match after.split_once(':') {
        Some((p, k)) => (
            p.to_string(),
            Some(k.split("__").next().unwrap_or("").to_string()),
        ),
        None => (after.to_string(), None),
    };
    CursorPosition::AfterColon {
        plugin_name: p_name,
        key: key.filter(|k| !k.is_empty()),
    }
}

fn find_value_in_attr(attr: Node, source: &str, offset: usize) -> Option<CursorPosition> {
    for i in 0..attr.child_count() {
        let child = attr.child(i as u32)?;
        match child.kind() {
            "attribute_value" | "quoted_attribute_value" => {
                if offset < child.start_byte() + 1 || offset > child.end_byte() - 1 {
                    return None;
                }
                let raw = child.utf8_text(source.as_bytes()).ok()?;
                let inner = raw[1..raw.len() - 1].to_string();
                return Some(CursorPosition::AttributeValue {
                    plugin_name: plugin_name(&attr_name_text(attr, source)?).to_string(),
                    value_start: child.start_byte() + 1,
                    full_value: inner,
                });
            }
            "string" | "jsx_expression" => {
                if offset < child.start_byte() + 1 || offset > child.end_byte() - 1 {
                    return None;
                }
                let raw = child.utf8_text(source.as_bytes()).ok()?;
                let inner = raw[1..raw.len() - 1].to_string();
                return Some(CursorPosition::AttributeValue {
                    plugin_name: plugin_name(&attr_name_text(attr, source)?).to_string(),
                    value_start: child.start_byte() + 1,
                    full_value: inner,
                });
            }
            _ => {}
        }
    }
    None
}

fn attr_name_text(attr: Node, source: &str) -> Option<String> {
    attr_name_child(attr)?
        .utf8_text(source.as_bytes())
        .ok()
        .map(String::from)
}

fn attr_name_child(attr: Node) -> Option<Node> {
    for i in 0..attr.child_count() {
        let child = attr.child(i as u32)?;
        match child.kind() {
            "attribute_name" | "property_identifier" | "jsx_namespace_name" => return Some(child),
            _ => {}
        }
    }
    None
}

/// Find the attribute node containing `offset`, or None if not in an attribute.
fn find_attr_node(root: Node, offset: usize) -> Option<Node> {
    let mut node = root;
    loop {
        if node.start_byte() > offset || node.end_byte() < offset {
            return None;
        }
        if node.kind() == "attribute" || node.kind() == "jsx_attribute" {
            return Some(node);
        }
        let mut found = false;
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                if child.start_byte() <= offset && child.end_byte() >= offset {
                    node = child;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return None;
        }
    }
}

fn is_in_markup(root: Node, offset: usize) -> bool {
    fn walk(node: Node, offset: usize) -> bool {
        if node.start_byte() > offset || node.end_byte() < offset {
            return false;
        }
        let kind = node.kind();
        if kind.starts_with("jsx_")
            || kind == "attribute"
            || kind == "start_tag"
            || kind == "element"
            || kind == "self_closing_tag"
        {
            return true;
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                if walk(child, offset) {
                    return true;
                }
            }
        }
        false
    }
    walk(root, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect_at(text: &str, cursor_offset: usize) -> CursorPosition {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_html::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(text, None).unwrap();
        detect(tree.root_node(), text, cursor_offset)
    }

    #[test]
    fn test_attr_name_no_colon() {
        let pos = detect_at(r#"<div data-show="true">"#, 7);
        assert!(matches!(pos, CursorPosition::AttributeName { .. }));
        if let CursorPosition::AttributeName { plugin_name } = &pos {
            assert_eq!(plugin_name, "show");
        }
    }

    #[test]
    fn test_attr_name_before_colon() {
        let pos = detect_at(r#"<div data-on:click="x">"#, 7);
        assert!(matches!(pos, CursorPosition::AttributeName { .. }));
    }

    #[test]
    fn test_after_colon() {
        let pos = detect_at(r#"<div data-on:click="x">"#, 13);
        assert!(matches!(pos, CursorPosition::AfterColon { .. }));
        if let CursorPosition::AfterColon { plugin_name, key } = &pos {
            assert_eq!(plugin_name, "on");
            assert_eq!(key.as_deref(), Some("click"));
        }
    }

    #[test]
    fn test_after_colon_modifiers() {
        let pos = detect_at(r#"<div data-on:click__debounce="x">"#, 20);
        assert!(matches!(pos, CursorPosition::AfterColon { .. }));
    }

    #[test]
    fn test_attr_value() {
        let pos = detect_at(r#"<div data-show="true">"#, 16);
        assert!(matches!(pos, CursorPosition::AttributeValue { .. }));
        if let CursorPosition::AttributeValue {
            plugin_name,
            full_value,
            ..
        } = &pos
        {
            assert_eq!(plugin_name, "show");
            assert_eq!(full_value, "true");
        }
    }

    #[test]
    fn test_in_markup() {
        let pos = detect_at(r#"<div class="foo">hello</div>"#, 5);
        assert!(matches!(pos, CursorPosition::InMarkup));
    }

    #[test]
    fn test_outside_markup() {
        let pos = detect_at(r#"<script>var x = 1;</script>"#, 10);
        assert!(matches!(pos, CursorPosition::None));
    }

    #[test]
    fn test_tsx_in_markup() {
        let text = r#"export function T() { return <div data-show="true">hi</div> }"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap();
        let tree = parser.parse(text, None).unwrap();
        let pos = detect(tree.root_node(), text, 38);
        assert!(matches!(pos, CursorPosition::AttributeName { .. }));
    }

    #[test]
    fn test_tsx_outside_markup() {
        let text = r#"export function T() { return <div data-show="true">hi</div> }"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap();
        let tree = parser.parse(text, None).unwrap();
        let pos = detect(tree.root_node(), text, 10);
        assert!(matches!(pos, CursorPosition::None));
    }
}
