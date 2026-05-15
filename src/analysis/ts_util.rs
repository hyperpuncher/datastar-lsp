use crate::attribute_name::parse_attribute_key;

/// Lightweight attribute info extracted from tree-sitter parse.
pub struct AttrData {
    pub raw_name: String,
    pub plugin_name: String,
    pub key: Option<String>,
    pub name_start: usize,
    pub name_len: usize,
    pub value: Option<String>,
    pub value_start: Option<usize>,
    pub modifiers: Vec<(String, Vec<String>)>,
    /// TSX: true when a bare `:` follows the attribute name as a sibling node.
    /// e.g. `<input data-bind: />` — the `:` is not in `property_identifier`.
    pub has_trailing_colon: bool,
}

/// Pick the tree-sitter language for a file URI.
pub fn language_for(uri: &tower_lsp::lsp_types::Url) -> tree_sitter::Language {
    let path = uri.path().to_lowercase();
    if path.ends_with(".jsx") || path.ends_with(".tsx") {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    } else {
        tree_sitter_html::LANGUAGE.into()
    }
}

/// Full parse + collect: create parser, set language, parse text, collect attrs.
pub fn parse_and_collect(
    text: &str,
    uri: &tower_lsp::lsp_types::Url,
) -> Option<(tree_sitter::Tree, Vec<AttrData>)> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language_for(uri)).ok()?;
    let tree = parser.parse(text, None)?;
    let attrs = collect_from_tree(tree.root_node(), text);
    Some((tree, attrs))
}

/// Collect all `data-*` attributes from a tree-sitter parse tree.
pub fn collect_from_tree(node: tree_sitter::Node, text: &str) -> Vec<AttrData> {
    let mut attrs = Vec::new();
    collect_recursive(node, text.as_bytes(), &mut attrs);
    attrs
}

fn collect_recursive(node: tree_sitter::Node, src: &[u8], attrs: &mut Vec<AttrData>) {
    match node.kind() {
        "attribute" | "jsx_attribute" => {
            // Check for `attrs={{...}}` — JSX pattern for passing data-* as object
            if let Some(items) = extract_attrs_prop(node, src) {
                attrs.extend(items);
                return;
            }
            if let Some(item) = extract_attr(node, src) {
                attrs.push(item);
                return;
            }
        }
        _ => {}
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            collect_recursive(child, src, attrs);
        }
    }
}

fn extract_attr(node: tree_sitter::Node, src: &[u8]) -> Option<AttrData> {
    let mut name: Option<String> = None;
    let mut name_start = 0usize;
    let mut value: Option<String> = None;
    let mut value_start: Option<usize> = None;

    for i in 0..node.child_count() {
        let child = node.child(i as u32)?;
        match child.kind() {
            // Attribute name — HTML uses "attribute_name", JSX uses these two
            "attribute_name" | "property_identifier" | "jsx_namespace_name" => {
                name_start = child.start_byte();
                name = child.utf8_text(src).ok().map(String::from);
            }
            // Simple quoted value — HTML (node text is "...content...")
            "attribute_value" | "quoted_attribute_value" => {
                let raw = child.utf8_text(src).ok()?;
                value = Some(raw[1..raw.len() - 1].to_string());
                value_start = Some(child.start_byte() + 1);
            }
            // JSX string value (includes quotes, extract inner fragment)
            "string" => {
                let raw = child.utf8_text(src).ok()?;
                // Look for inner string_fragment for exact byte position
                for j in 0..child.child_count() {
                    if let Some(frag) = child.child(j as u32) {
                        if frag.kind() == "string_fragment" {
                            value = frag.utf8_text(src).ok().map(String::from);
                            value_start = Some(frag.start_byte());
                            break;
                        }
                    }
                }
                // Fallback: strip quotes manually
                if value.is_none() {
                    value = Some(raw[1..raw.len() - 1].to_string());
                    value_start = Some(child.start_byte() + 1);
                }
            }
            // JSX expression (template literal or JS): `{...}`
            "jsx_expression" => {
                let raw = child.utf8_text(src).ok()?;
                value = Some(raw[1..raw.len() - 1].to_string());
                value_start = Some(child.start_byte() + 1);
            }
            _ => {}
        }
    }

    let name = name?;
    if !name.starts_with("data-") {
        return None;
    }
    let parsed = parse_attribute_key(&name);

    // Detect TSX trailing colon: check if next sibling of the attribute node is `:` or ERROR `:`
    let has_trailing_colon = node
        .next_sibling()
        .map(|sib| {
            let txt = sib.utf8_text(src).ok().unwrap_or("");
            txt == ":" || sib.kind() == "ERROR"
        })
        .unwrap_or(false);

    Some(AttrData {
        name_len: name.len(),
        raw_name: name.clone(),
        plugin_name: parsed.plugin,
        key: parsed.key,
        name_start,
        value,
        value_start,
        modifiers: parsed.modifiers,
        has_trailing_colon,
    })
}

/// Extract data-* attributes from `attrs={{...}}` JSX prop.
/// Walks the object literal pairs, treating string keys starting with `data-` as attributes.
fn extract_attrs_prop(node: tree_sitter::Node, src: &[u8]) -> Option<Vec<AttrData>> {
    // Check if this is an `attrs` prop
    let mut name_node: Option<tree_sitter::Node> = None;
    let mut expr_node: Option<tree_sitter::Node> = None;
    for i in 0..node.child_count() {
        let child = node.child(i as u32)?;
        match child.kind() {
            "property_identifier" | "jsx_namespace_name" => name_node = Some(child),
            "jsx_expression" => expr_node = Some(child),
            _ => {}
        }
    }
    let name = name_node?.utf8_text(src).ok()?;
    if name != "attrs" {
        return None;
    }

    // Find the object literal inside the jsx_expression
    let expr = expr_node?;
    // jsx_expression = `{...}` — find the inner object node
    let mut obj: Option<tree_sitter::Node> = None;
    for i in 0..expr.child_count() {
        if let Some(child) = expr.child(i as u32) {
            if child.kind() == "object" {
                obj = Some(child);
                break;
            }
        }
    }
    let obj = obj?;

    let mut attrs = Vec::new();
    for i in 0..obj.child_count() {
        let pair = match obj.child(i as u32) {
            Some(p) if p.kind() == "pair" => p,
            _ => continue,
        };
        // pair: key (string/property_identifier) : value
        let mut key_text: Option<String> = None;
        let mut key_start: Option<usize> = None;
        let mut val_text: Option<String> = None;
        let mut val_start: Option<usize> = None;
        for j in 0..pair.child_count() {
            let child = pair.child(j as u32)?;
            match child.kind() {
                "string" | "template_string" => {
                    let raw = child.utf8_text(src).ok()?;
                    if key_text.is_none() {
                        // Key: strip quotes
                        let inner = if raw.len() >= 2 {
                            raw[1..raw.len() - 1].to_string()
                        } else {
                            raw.to_string()
                        };
                        key_text = Some(inner);
                        key_start = Some(child.start_byte() + 1);
                    } else {
                        // Value: strip quotes/template
                        let inner = if raw.len() >= 2 {
                            raw[1..raw.len() - 1].to_string()
                        } else {
                            raw.to_string()
                        };
                        val_text = Some(inner);
                        val_start = Some(child.start_byte() + 1);
                    }
                }
                "property_identifier" if key_text.is_none() => {
                    key_text = child.utf8_text(src).ok().map(String::from);
                    key_start = Some(child.start_byte());
                }
                "jsx_expression" | "template_literal" => {
                    let raw = child.utf8_text(src).ok()?;
                    val_text = Some(raw[1..raw.len() - 1].to_string());
                    val_start = Some(child.start_byte() + 1);
                }
                "number" | "true" | "false" | "null" | "undefined" => {
                    val_text = child.utf8_text(src).ok().map(String::from);
                    val_start = Some(child.start_byte());
                }
                _ => {}
            }
        }

        let key = key_text?;
        if !key.starts_with("data-") {
            continue;
        }
        let parsed = parse_attribute_key(&key);
        let name_start = key_start.unwrap_or(0);
        attrs.push(AttrData {
            name_len: key.len(),
            raw_name: key.clone(),
            plugin_name: parsed.plugin,
            key: parsed.key,
            name_start,
            value: val_text,
            value_start: val_start,
            modifiers: parsed.modifiers,
            has_trailing_colon: false,
        });
    }

    if attrs.is_empty() {
        None
    } else {
        Some(attrs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_html() {
        let text = r#"<div data-show="true"><button data-on:click="$counter++">+</button></div>"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_html::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(text, None).unwrap();
        let attrs = collect_from_tree(tree.root_node(), text);
        assert_eq!(attrs.len(), 2);
    }

    #[test]
    fn test_collect_tsx() {
        let text = r#"export function Test() { return <div data-show="true"><button data-on:click="$counter++">+</button></div> }"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap();
        let tree = parser.parse(text, None).unwrap();
        let attrs = collect_from_tree(tree.root_node(), text);
        assert!(attrs.len() >= 2, "got {}", attrs.len());
    }

    #[test]
    fn test_collect_tsx_template_literal() {
        let text = r#"export function T() { return <button data-on:click={`@get('/api?p=${n}`)}>Go</button> }"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap();
        let tree = parser.parse(text, None).unwrap();
        let attrs = collect_from_tree(tree.root_node(), text);
        assert!(!attrs.is_empty(), "got {}", attrs.len());
    }

    #[test]
    fn test_language_for() {
        let html_uri = tower_lsp::lsp_types::Url::parse("file:///tmp/test.html").unwrap();
        let tsx_uri = tower_lsp::lsp_types::Url::parse("file:///tmp/test.tsx").unwrap();
        assert!(language_for(&html_uri) == tree_sitter_html::LANGUAGE.into());
        assert!(language_for(&tsx_uri) == tree_sitter_typescript::LANGUAGE_TSX.into());
    }

    #[test]
    fn test_collect_attrs_prop() {
        let text = r#"export function T() { return <div attrs={{"data-on:input__debounce.200ms": "@post('/filters')", "data-show": "$open"}} /> }"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap();
        let tree = parser.parse(text, None).unwrap();
        let attrs = collect_from_tree(tree.root_node(), text);
        assert!(
            attrs.len() >= 2,
            "expected 2+ attrs from attrs prop, got {}: {:?}",
            attrs.len(),
            attrs.iter().map(|a| &a.raw_name).collect::<Vec<_>>()
        );
        let on_attr = attrs.iter().find(|a| a.plugin_name == "on").unwrap();
        assert_eq!(on_attr.key.as_deref(), Some("input"));
        assert_eq!(on_attr.modifiers.len(), 1);
        assert_eq!(on_attr.modifiers[0].0, "debounce");
        assert_eq!(on_attr.modifiers[0].1, vec!["200ms"]);
        assert!(attrs.iter().any(|a| a.plugin_name == "show"));
    }
}
