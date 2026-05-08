use anyhow::Result;
use tree_sitter::{Node, Parser, Tree};

/// Represents a parsed `data-*` attribute from HTML/JSX/TSX.
#[derive(Debug, Clone)]
pub struct DataAttribute {
    /// The full attribute name (e.g. "data-on:click__debounce.500ms")
    pub raw_name: String,
    /// The plugin name (e.g. "on")
    pub plugin_name: String,
    /// The key suffix after `:` (e.g. "click"), if present
    pub key: Option<String>,
    /// Modifier keys with their tags (e.g. [("debounce", ["500ms"])])
    pub modifiers: Vec<(String, Vec<String>)>,
    /// The attribute value string
    pub value: Option<String>,
    /// Byte offset of the attribute name in source
    pub name_start: usize,
    /// Byte offset of the attribute value in source
    pub value_start: Option<usize>,
}

/// Supported languages for HTML-like parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Html,
    Jsx,
    Tsx,
}

impl Language {
    pub fn from_file_extension(path: &str) -> Option<Self> {
        if path.ends_with(".html")
            || path.ends_with(".htm")
            || path.ends_with(".templ")
            || path.ends_with(".heex")
            || path.ends_with(".blade")
        {
            Some(Language::Html)
        } else if path.ends_with(".jsx") {
            Some(Language::Jsx)
        } else if path.ends_with(".tsx") {
            Some(Language::Tsx)
        } else {
            // Default: try HTML, then TSX
            None
        }
    }
}

/// Parses source and returns all Datastar data-* attributes found.
pub fn parse_html(source: &[u8]) -> Result<(Tree, Vec<DataAttribute>)> {
    parse_with(source, tree_sitter_html::LANGUAGE.into())
}

/// Parse JSX source (also works for TSX).
pub fn parse_jsx(source: &[u8]) -> Result<(Tree, Vec<DataAttribute>)> {
    parse_with(source, tree_sitter_typescript::LANGUAGE_TSX.into())
}

/// Parse with a specific tree-sitter language.
pub fn parse_with(
    source: &[u8],
    language: tree_sitter::Language,
) -> Result<(Tree, Vec<DataAttribute>)> {
    let mut parser = Parser::new();
    parser.set_language(&language)?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("Failed to parse with language"))?;
    let attrs = extract_attributes(&tree, source);

    Ok((tree, attrs))
}

/// Walk the CST and collect all `data-*` attributes.
fn extract_attributes(tree: &Tree, source: &[u8]) -> Vec<DataAttribute> {
    let mut attrs = Vec::new();
    let root = tree.root_node();
    walk_for_data_attrs(root, source, &mut attrs);
    attrs
}

fn walk_for_data_attrs(node: Node, source: &[u8], attrs: &mut Vec<DataAttribute>) {
    // Check for HTML-style attributes
    if node.kind() == "attribute" {
        if let Some(data_attr) = try_extract_data_attr(node, source) {
            attrs.push(data_attr);
            return; // Don't recurse into attribute children
        }
    }

    // Check for JSX-style attributes
    if node.kind() == "jsx_attribute" {
        if let Some(data_attr) = try_extract_jsx_attr(node, source) {
            attrs.push(data_attr);
            return;
        }
    }

    // Walk children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            walk_for_data_attrs(child, source, attrs);
        }
    }
}

fn try_extract_data_attr(node: Node, source: &[u8]) -> Option<DataAttribute> {
    let mut name_node: Option<Node> = None;
    let mut value_node: Option<Node> = None;

    for i in 0..node.child_count() {
        let child = node.child(i as u32)?;
        match child.kind() {
            "attribute_name" => name_node = Some(child),
            "attribute_value" | "quoted_attribute_value" => value_node = Some(child),
            _ => {}
        }
    }

    let name_node = name_node?;
    let raw_name = name_node.utf8_text(source).ok()?;

    if !raw_name.starts_with("data-") {
        return None;
    }

    let parsed = parse_attribute_key(raw_name);
    let value = value_node
        .and_then(|v| v.utf8_text(source).ok())
        .map(|s| s.trim_matches(&['"', '\''] as &[_]).to_string());

    Some(DataAttribute {
        raw_name: raw_name.to_string(),
        plugin_name: parsed.plugin,
        key: parsed.key,
        modifiers: parsed.modifiers,
        value,
        name_start: name_node.start_byte(),
        value_start: value_node.map(|v| v.start_byte()),
    })
}

fn try_extract_jsx_attr(node: Node, source: &[u8]) -> Option<DataAttribute> {
    let mut name_node: Option<Node> = None;
    let mut value_text: Option<String> = None;
    let mut value_start: Option<usize> = None;

    for i in 0..node.child_count() {
        let child = node.child(i as u32)?;
        match child.kind() {
            // Regular JSX attribute: property_identifier
            "property_identifier" => {
                let raw = child.utf8_text(source).ok()?;
                if raw.starts_with("data-") {
                    name_node = Some(child);
                }
            }
            // Namespaced JSX attribute: jsx_namespace_name (e.g. data-bind:foo)
            // These come as a "jsx_namespace_name" node with text "data-bind:foo"
            "jsx_namespace_name" => {
                name_node = Some(child);
            }
            // String value
            "string" => {
                if let Some(frag) = child.child_by_field_name("value") {
                    value_text = frag.utf8_text(source).ok().map(String::from);
                } else {
                    value_text = child.utf8_text(source).ok().map(String::from);
                }
                value_start = Some(child.start_byte());
            }
            // Template expression value: {`...`} or {"..."}
            "jsx_expression" => {
                // Look inside for template_string or string
                for j in 0..child.child_count() {
                    let inner = child.child(j as u32)?;
                    match inner.kind() {
                        "template_string" | "string" => {
                            // Get the string_fragment inside
                            for k in 0..inner.child_count() {
                                let frag = inner.child(k as u32)?;
                                if frag.kind() == "string_fragment" {
                                    value_text = frag.utf8_text(source).ok().map(String::from);
                                    value_start = Some(inner.start_byte());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    let name_node = name_node?;
    let raw_name = name_node.utf8_text(source).ok()?;

    if !raw_name.starts_with("data-") {
        return None;
    }

    let parsed = parse_attribute_key(raw_name);
    let value = value_text.map(|s| s.trim_matches(&['"', '\''] as &[_]).trim().to_string());

    Some(DataAttribute {
        raw_name: raw_name.to_string(),
        plugin_name: parsed.plugin,
        key: parsed.key,
        modifiers: parsed.modifiers,
        value,
        name_start: name_node.start_byte(),
        value_start,
    })
}

struct ParsedKey {
    plugin: String,
    key: Option<String>,
    modifiers: Vec<(String, Vec<String>)>,
}

/// Parse a `data-*` attribute name into plugin, key, and modifiers.
fn parse_attribute_key(raw: &str) -> ParsedKey {
    let rest = &raw[5..];

    let (base, modifier_str) = match rest.find("__") {
        Some(pos) => (&rest[..pos], Some(&rest[pos + 2..])),
        None => (rest, None),
    };

    let (plugin, key) = match base.find(':') {
        Some(pos) => (&base[..pos], Some(base[pos + 1..].to_string())),
        None => (base, None),
    };

    let modifiers = modifier_str
        .unwrap_or("")
        .split("__")
        .filter(|s| !s.is_empty())
        .filter_map(|mod_part| {
            let mut parts = mod_part.splitn(2, '.');
            let key = parts.next()?.to_string();
            let tags: Vec<String> = parts
                .flat_map(|s| s.split('.'))
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            Some((key, tags))
        })
        .collect();

    ParsedKey {
        plugin: plugin.to_string(),
        key,
        modifiers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_attribute_key_simple() {
        let p = parse_attribute_key("data-show");
        assert_eq!(p.plugin, "show");
        assert!(p.key.is_none());
        assert!(p.modifiers.is_empty());
    }

    #[test]
    fn test_parse_attribute_key_with_key() {
        let p = parse_attribute_key("data-on:click");
        assert_eq!(p.plugin, "on");
        assert_eq!(p.key.as_deref(), Some("click"));
        assert!(p.modifiers.is_empty());
    }

    #[test]
    fn test_parse_attribute_key_with_modifiers() {
        let p = parse_attribute_key("data-on:click__debounce.500ms.leading");
        assert_eq!(p.plugin, "on");
        assert_eq!(p.key.as_deref(), Some("click"));
        assert_eq!(p.modifiers.len(), 1);
        assert_eq!(p.modifiers[0].0, "debounce");
        assert_eq!(p.modifiers[0].1, vec!["500ms", "leading"]);
    }

    #[test]
    fn test_parse_attribute_key_multiple_modifiers() {
        let p = parse_attribute_key("data-on:click__window__debounce.500ms");
        assert_eq!(p.plugin, "on");
        assert_eq!(p.key.as_deref(), Some("click"));
        assert_eq!(p.modifiers.len(), 2);
        assert_eq!(p.modifiers[0].0, "window");
        assert_eq!(p.modifiers[1].0, "debounce");
    }

    #[test]
    fn test_parse_attribute_key_no_key_with_modifier() {
        let p = parse_attribute_key("data-on-raf__throttle.10ms");
        assert_eq!(p.plugin, "on-raf");
        assert!(p.key.is_none());
        assert_eq!(p.modifiers.len(), 1);
        assert_eq!(p.modifiers[0].0, "throttle");
        assert_eq!(p.modifiers[0].1, vec!["10ms"]);
    }

    #[test]
    fn test_parse_html_simple() {
        let (tree, attrs) = parse_html(b"<div data-show=\"$foo\"></div>").unwrap();
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].plugin_name, "show");
        assert_eq!(attrs[0].value.as_deref(), Some("$foo"));

        let root = tree.root_node();
        assert!(root.kind() == "document" || root.kind() == "fragment");
    }

    #[test]
    fn test_parse_html_multiple_attrs() {
        let (_, attrs) = parse_html(
            br#"<div data-signals:foo="1" data-bind:bar data-show="$foo"><button data-on:click__debounce.500ms="@post('/endpoint')">Click</button></div>"#,
        )
        .unwrap();
        assert_eq!(attrs.len(), 4);

        let names: Vec<_> = attrs.iter().map(|a| a.plugin_name.as_str()).collect();
        assert!(names.contains(&"signals"));
        assert!(names.contains(&"bind"));
        assert!(names.contains(&"on"));
    }

    #[test]
    fn test_parse_jsx_attrs() {
        let source = br#"<input data-bind:q value="search" /><div data-show="$visible"></div>"#;
        let (_, attrs) = parse_jsx(source).unwrap();
        assert!(attrs.len() >= 1, "got {} attrs: {:?}", attrs.len(), attrs);
        let names: Vec<_> = attrs.iter().map(|a| a.plugin_name.as_str()).collect();
        assert!(names.contains(&"bind"), "expected bind in {:?}", names);
    }
}
