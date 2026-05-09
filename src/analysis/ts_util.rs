use crate::parser::html::parse_attribute_key;

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
}

/// Pick the tree-sitter language for a file URI.
/// HTML grammar works for most template languages; TypeScript for JSX/TSX.
pub fn language_for(uri: &tower_lsp::lsp_types::Url) -> tree_sitter::Language {
    let path = uri.path().to_lowercase();
    if path.ends_with(".jsx") || path.ends_with(".tsx") {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    } else {
        tree_sitter_html::LANGUAGE.into()
    }
}

/// Collect all `data-*` attributes from a tree-sitter HTML parse tree.
pub fn collect_from_tree(node: tree_sitter::Node, text: &str) -> Vec<AttrData> {
    let mut attrs = Vec::new();
    collect_recursive(node, text.as_bytes(), &mut attrs);
    attrs
}

fn collect_recursive(node: tree_sitter::Node, src: &[u8], attrs: &mut Vec<AttrData>) {
    if node.kind() == "attribute" {
        if let Some(item) = extract_one(node, src) {
            attrs.push(item);
            return;
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            collect_recursive(child, src, attrs);
        }
    }
}

fn extract_one(node: tree_sitter::Node, src: &[u8]) -> Option<AttrData> {
    let mut name: Option<String> = None;
    let mut name_start = 0usize;
    let mut value: Option<String> = None;
    let mut value_start: Option<usize> = None;

    for i in 0..node.child_count() {
        let child = node.child(i as u32)?;
        match child.kind() {
            "attribute_name" => {
                name_start = child.start_byte();
                name = child.utf8_text(src).ok().map(String::from);
            }
            "attribute_value" | "quoted_attribute_value" => {
                let raw = child.utf8_text(src).ok()?;
                value = Some(raw.trim_matches(&['"', '\''] as &[_]).to_string());
                value_start = Some(child.start_byte());
            }
            _ => {}
        }
    }

    let name = name?;
    if !name.starts_with("data-") {
        return None;
    }
    let parsed = parse_attribute_key(&name);
    Some(AttrData {
        name_len: name.len(),
        raw_name: name.clone(),
        plugin_name: parsed.plugin,
        key: parsed.key,
        name_start,
        value,
        value_start,
        modifiers: parsed.modifiers,
    })
}
