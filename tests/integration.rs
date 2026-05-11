use std::path::Path;

use datastar_lsp::attribute_name::parse_attribute_key;

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

// ── End-to-end completions tests ──

use datastar_lsp::analysis::completions;
use datastar_lsp::line_index::LineIndex;
use tower_lsp::lsp_types::{Position, Url};

/// Helper: get completions with cursor placed at the END of the pattern match.
fn complete_after(text: &str, uri: &str, pattern: &str) -> Vec<String> {
    let line_index = LineIndex::new(text.to_string());
    let match_start = text.find(pattern).unwrap_or_else(|| panic!("pattern not found: {pattern}"));
    let byte_offset = match_start + pattern.len();
    complete_at_byte(text, uri, &line_index, byte_offset)
}

/// Helper: get completions with cursor placed at a specific byte offset.
fn complete_at_byte(text: &str, uri: &str, line_index: &LineIndex, byte_offset: usize) -> Vec<String> {
    let (line, col) = line_index.byte_to_position(byte_offset);
    let url = Url::parse(uri).unwrap();
    let items = completions::generate(
        line_index,
        text,
        Position { line, character: col },
        &url,
    );
    items.into_iter().map(|i| i.label).collect()
}

#[test]
fn html_completion_after_data_hyphen() {
    let html = r#"<div data-></div>"#;
    let labels = complete_after(html, "file:///test.html", "data-");
    assert!(labels.contains(&"data-on".to_string()), "got: {labels:?}");
    assert!(labels.contains(&"data-text".to_string()), "got: {labels:?}");
}

#[test]
fn html_completion_after_data_on_colon() {
    let html = r#"<button data-on:></button>"#;
    let labels = complete_after(html, "file:///test.html", "data-on:");
    assert!(labels.contains(&"click".to_string()), "expected event names, got: {labels:?}");
}

#[test]
fn html_completion_modifiers_after_underscore() {
    let html = r#"<button data-on:click__></button>"#;
    let idx = html.find("click__").unwrap() + "click__".len();
    let li = LineIndex::new(html.to_string());
    let labels = complete_at_byte(html, "file:///test.html", &li, idx);
    assert!(labels.contains(&"__debounce".to_string()), "should have modifiers, got: {labels:?}");
}

#[test]
fn html_completion_in_value_dollar() {
    let html = r#"<div data-signals:count="0"><span data-text="$"></span></div>"#;
    let labels = complete_after(html, "file:///test.html", "$");
    assert!(labels.contains(&"$count".to_string()), "got: {labels:?}");
}

#[test]
fn html_no_completion_outside_markup() {
    let html = r#"<script>var x = data-</script>"#;
    let labels = complete_after(html, "file:///test.html", "data-");
    assert!(!labels.iter().any(|l| l.starts_with("data-")), "should not complete in script, got: {labels:?}");
}

#[test]
fn tsx_completion_after_data_hyphen() {
    let tsx = r#"export function T() { return <div data-></div> }"#;
    let labels = complete_after(tsx, "file:///test.tsx", "data-");
    assert!(labels.contains(&"data-on".to_string()), "got: {labels:?}");
}

#[test]
fn tsx_completion_after_data_on_colon() {
    let tsx = r#"export function T() { return <button data-on:></button> }"#;
    let labels = {
    let li = LineIndex::new(tsx.to_string());
    let colon = tsx.find("data-on:").unwrap() + 7;
    complete_at_byte(tsx, "file:///test.tsx", &li, colon)
};
    assert!(
        labels.contains(&"click".to_string()),
        "TSX data-on: should complete event names, got: {labels:?}"
    );
}

#[test]
fn tsx_no_completion_in_plain_typescript() {
    let tsx = r#"export function T() { const x = data-; return <div></div> }"#;
    let labels = complete_after(tsx, "file:///test.tsx", "data-");
    assert!(!labels.iter().any(|l| l.starts_with("data-")), "should not complete in TS code, got: {labels:?}");
}

#[test]
fn tsx_completion_evt_props_keydown() {
    let tsx = r#"export function T() { return <input data-on:keydown="evt." /> }"#;
    let labels = complete_after(tsx, "file:///test.tsx", "evt.");
    // KeyboardEvent should include 'key' property
    assert!(labels.iter().any(|l| l == "evt.key"), "should have KeyboardEvent props, got: {labels:?}");
}

#[test]
fn tsx_completion_evt_props_click() {
    let tsx = r#"export function T() { return <button data-on:click="evt."></button> }"#;
    let labels = complete_after(tsx, "file:///test.tsx", "evt.");
    // MouseEvent should include 'clientX'
    assert!(labels.iter().any(|l| l == "evt.clientX"), "should have MouseEvent props, got: {labels:?}");
}

#[test]
fn tsx_completion_modifiers_after_underscore() {
    let tsx = r#"export function T() { return <button data-on:click__></button> }"#;
    let idx = tsx.find("click__").unwrap() + "click__".len();
    let li = LineIndex::new(tsx.to_string());
    let labels = complete_at_byte(tsx, "file:///test.tsx", &li, idx);
    assert!(labels.contains(&"__debounce".to_string()), "should have modifiers, got: {labels:?}");
}

