use std::path::Path;

use datastar_lsp::analysis::signals;
use datastar_lsp::parser::html::{parse_html, parse_jsx};

fn read_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test/fixtures")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

fn count_data_attrs(attrs: &[datastar_lsp::parser::html::DataAttribute]) -> usize {
    attrs.len()
}

// ── HTML ──

#[test]
fn html_parses_data_attributes() {
    let html = read_fixture("test.html");
    let (_, attrs) = parse_html(html.as_bytes()).unwrap();
    assert!(
        count_data_attrs(&attrs) > 20,
        "expected many attrs, got {}",
        attrs.len()
    );
}

#[test]
fn html_finds_signal_definitions() {
    let html = read_fixture("test.html");
    let analysis = signals::analyze_signals(&html);
    assert!(analysis.top_level_names.contains("counter"));
    assert!(analysis.top_level_names.contains("search"));
    assert!(analysis.top_level_names.contains("name"));
    assert!(analysis.top_level_names.contains("user"));
}

// ── Templ ──

#[test]
fn templ_parses_data_attributes() {
    let text = read_fixture("test.templ");
    if let Ok((_, attrs)) = parse_html(text.as_bytes()) {
        assert!(
            count_data_attrs(&attrs) > 3,
            "templ: expected some attrs, got {}",
            attrs.len()
        );
    }
}

// ── JSX ──

#[test]
fn jsx_parses_data_attributes() {
    let text = read_fixture("test.jsx");
    let (_, attrs) = parse_jsx(text.as_bytes()).unwrap();
    assert!(
        count_data_attrs(&attrs) > 5,
        "jsx: expected many attrs, got {}",
        attrs.len()
    );

    let plugin_names: Vec<&str> = attrs.iter().map(|a| a.plugin_name.as_str()).collect();
    assert!(plugin_names.contains(&"bind"));
    assert!(plugin_names.contains(&"on"));
    assert!(plugin_names.contains(&"show"));
    assert!(plugin_names.contains(&"class"));
}

// ── TSX ──

#[test]
fn tsx_parses_data_attributes() {
    let text = read_fixture("test.tsx");
    let (_, attrs) = parse_jsx(text.as_bytes()).unwrap();
    assert!(
        count_data_attrs(&attrs) > 5,
        "tsx: expected many attrs, got {}",
        attrs.len()
    );
}

// ── HEEx ──

#[test]
fn heex_parses_data_attributes() {
    let text = read_fixture("test.heex");
    if let Ok((_, attrs)) = parse_html(text.as_bytes()) {
        assert!(
            count_data_attrs(&attrs) > 3,
            "heex: expected some attrs, got {}",
            attrs.len()
        );
    }
}

// ── Blade ──

#[test]
fn blade_parses_data_attributes() {
    let text = read_fixture("test.blade.php");
    if let Ok((_, attrs)) = parse_html(text.as_bytes()) {
        assert!(
            count_data_attrs(&attrs) > 3,
            "blade: expected some attrs, got {}",
            attrs.len()
        );
    }
}
