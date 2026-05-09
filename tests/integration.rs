use std::path::Path;

use datastar_lsp::parser::html::parse_attribute_key;

fn read_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test/fixtures")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

fn clean_attr_name(raw: &str) -> String {
    let s = raw.trim_start_matches(['"', '\'', '`']);
    let s = s.trim_end_matches(['"', '\'', '`', '=']);
    if let Some(p) = s.rfind('=') {
        s[..p].to_string()
    } else {
        s.to_string()
    }
}

fn count_data_attrs(text: &str) -> usize {
    let mut count = 0;
    for chunk in text.split('>') {
        for part in chunk.split_whitespace() {
            if clean_attr_name(part).starts_with("data-") {
                count += 1;
            }
        }
    }
    count
}

fn collect_plugin_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for chunk in text.split('>') {
        for part in chunk.split_whitespace() {
            let cleaned = clean_attr_name(part);
            if cleaned.starts_with("data-") {
                let parsed = parse_attribute_key(&cleaned);
                names.push(parsed.plugin);
            }
        }
    }
    names
}

fn collect_signal_definitions(text: &str) -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    for prefix in &[
        "data-signals:",
        "data-bind:",
        "data-computed:",
        "data-ref:",
        "data-indicator:",
        "data-match-media:",
    ] {
        for (pos, _) in text.match_indices(prefix) {
            let after = &text[pos + prefix.len()..];
            if after.starts_with('{') || after.starts_with('"') {
                continue;
            }
            let end = after
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.')
                .unwrap_or(after.len());
            if end > 0 {
                names.insert(after[..end].to_string());
            }
        }
    }
    names
}

#[test]
fn html_parses_data_attributes() {
    let html = read_fixture("test.html");
    assert!(count_data_attrs(&html) > 20);
}

#[test]
fn html_finds_signal_definitions() {
    let html = read_fixture("test.html");
    let defs = collect_signal_definitions(&html);
    assert!(defs.contains("counter"));
    assert!(defs.contains("search"));
    assert!(defs.contains("name"));
}

#[test]
fn templ_parses_data_attributes() {
    let text = read_fixture("test.templ");
    assert!(count_data_attrs(&text) > 3);
}

#[test]
fn jsx_parses_data_attributes() {
    let text = read_fixture("test.jsx");
    let names = collect_plugin_names(&text);
    assert!(names.len() > 5);
    assert!(names.contains(&"bind".to_string()));
    assert!(names.contains(&"show".to_string()));
}

#[test]
fn tsx_parses_data_attributes() {
    let text = read_fixture("test.tsx");
    assert!(collect_plugin_names(&text).len() > 5);
}

#[test]
fn heex_parses_data_attributes() {
    let text = read_fixture("test.heex");
    assert!(count_data_attrs(&text) > 3);
}

#[test]
fn blade_parses_data_attributes() {
    let text = read_fixture("test.blade.php");
    assert!(count_data_attrs(&text) > 3);
}
