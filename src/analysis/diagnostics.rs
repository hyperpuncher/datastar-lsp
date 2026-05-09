use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Url};

use crate::analysis::signal_util::{self, DEFINERS};
use crate::analysis::ts_util;
use crate::data::{actions, attributes, modifiers};
use crate::line_index::LineIndex;
use crate::util::byte_range_to_lsp_range;

/// Generate diagnostics by parsing the document with tree-sitter and walking data-* attrs.
pub fn generate(
    line_index: &LineIndex,
    text: &str,
    uri: &Url,
    project_index: Option<&crate::analysis::project_index::ProjectIndex>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let Some((_, attrs)) = ts_util::parse_and_collect(text, uri) else {
        return diagnostics;
    };

    let attr_registry = attributes::all();
    let action_registry = actions::all();
    let modifier_registry = modifiers::all();

    let defined_signals: std::collections::BTreeSet<String> = attrs
        .iter()
        .filter(|a| DEFINERS.contains(&a.plugin_name.as_str()))
        .filter_map(|a| a.key.clone())
        .collect();

    for attr in &attrs {
        check_attribute_validity(
            attr,
            &attr_registry,
            &modifier_registry,
            line_index,
            &mut diagnostics,
        );
        check_value_actions(attr, &action_registry, line_index, &mut diagnostics);
        check_value_signals(attr, line_index, &mut diagnostics, &defined_signals);
    }

    // Cross-file signal check
    if project_index.is_some() {
        for attr in &attrs {
            emit_undefined_signals(
                attr,
                line_index,
                &mut diagnostics,
                &defined_signals,
                project_index,
            );
        }
    }

    diagnostics
}

fn check_attribute_validity(
    attr: &crate::analysis::ts_util::AttrData,
    registry: &std::collections::BTreeMap<&str, attributes::AttributeDef>,
    modifier_registry: &std::collections::BTreeMap<&str, modifiers::ModifierDef>,
    line_index: &LineIndex,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let def = match registry.get(attr.plugin_name.as_str()) {
        Some(d) => d,
        None => {
            let range = byte_range_to_lsp_range(
                line_index,
                attr.name_start,
                attr.name_start + attr.name_len,
            );
            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("datastar".to_string()),
                message: format!(
                    "Unknown Datastar attribute: 'data-{}' is not a recognized plugin.",
                    attr.plugin_name
                ),
                ..Default::default()
            });
            return;
        }
    };

    match (def.key_req, &attr.key) {
        (attributes::Requirement::Must, None) => {
            let range = byte_range_to_lsp_range(
                line_index,
                attr.name_start,
                attr.name_start + attr.name_len,
            );
            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("datastar".to_string()),
                message: format!("Missing key: 'data-{}' requires a key.", attr.plugin_name),
                ..Default::default()
            });
        }
        (attributes::Requirement::Denied, Some(key)) => {
            let pos = attr.raw_name.find(':').unwrap_or(0);
            let range = byte_range_to_lsp_range(
                line_index,
                attr.name_start + pos,
                attr.name_start + pos + 1 + key.len(),
            );
            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("datastar".to_string()),
                message: format!(
                    "Unexpected key: 'data-{}' does not accept a key.",
                    attr.plugin_name
                ),
                ..Default::default()
            });
        }
        (attributes::Requirement::Exclusive, Some(_)) if attr.value.is_some() => {
            let range = byte_range_to_lsp_range(
                line_index,
                attr.name_start,
                attr.name_start + attr.name_len,
            );
            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("datastar".to_string()),
                message: format!(
                    "Exclusive attribute: 'data-{}' accepts either a key or a value, not both.",
                    attr.plugin_name
                ),
                ..Default::default()
            });
        }
        _ => {}
    }

    match (def.value_req, &attr.value) {
        (attributes::Requirement::Must, None) => {
            let range = byte_range_to_lsp_range(
                line_index,
                attr.name_start,
                attr.name_start + attr.name_len,
            );
            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("datastar".to_string()),
                message: format!(
                    "Missing value: 'data-{}' requires a value expression.",
                    attr.plugin_name
                ),
                ..Default::default()
            });
        }
        (attributes::Requirement::Denied, Some(_)) => {
            let start = attr.value_start.unwrap_or(attr.name_start);
            let end = attr
                .value_start
                .map(|s| s + attr.value.as_ref().map(|v| v.len()).unwrap_or(0))
                .unwrap_or(attr.name_start + attr.name_len);
            let range = byte_range_to_lsp_range(line_index, start, end);
            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::WARNING),
                source: Some("datastar".to_string()),
                message: format!(
                    "Unexpected value: 'data-{}' does not accept a value.",
                    attr.plugin_name
                ),
                ..Default::default()
            });
        }
        _ => {}
    }

    for (mod_key, _tags) in &attr.modifiers {
        if !modifier_registry.contains_key(mod_key.as_str()) {
            if let Some(pos) = attr.raw_name.find(&format!("__{mod_key}")) {
                let start = attr.name_start + pos;
                let end = start + 2 + mod_key.len();
                let range = byte_range_to_lsp_range(line_index, start, end);
                diagnostics.push(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some("datastar".to_string()),
                    message: format!(
                        "Unknown modifier: '__{mod_key}' is not a recognized modifier."
                    ),
                    ..Default::default()
                });
            }
            continue;
        }
        if !def.modifier_keys.contains(&mod_key.as_str())
            && !signal_util::is_global_modifier(mod_key)
        {
            if let Some(pos) = attr.raw_name.find(&format!("__{mod_key}")) {
                let start = attr.name_start + pos;
                let end = start + 2 + mod_key.len();
                let range = byte_range_to_lsp_range(line_index, start, end);
                diagnostics.push(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some("datastar".to_string()),
                    message: format!(
                        "Modifier '__{mod_key}' is not valid for 'data-{}'.",
                        attr.plugin_name
                    ),
                    ..Default::default()
                });
            }
        }
    }
}

fn check_value_actions(
    attr: &crate::analysis::ts_util::AttrData,
    registry: &std::collections::BTreeMap<&str, actions::ActionDef>,
    line_index: &LineIndex,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let value = match &attr.value {
        Some(v) => v,
        None => return,
    };
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next.is_ascii_alphabetic() || next == b'_' {
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                let name = std::str::from_utf8(&bytes[i + 1..j]).unwrap_or("");
                if !name.is_empty()
                    && !registry.contains_key(name)
                    && !name.starts_with(|c: char| c.is_ascii_uppercase())
                {
                    let value_start = attr.value_start.unwrap_or(0);
                    let start = value_start + i;
                    let end = start + (j - i);
                    let range = byte_range_to_lsp_range(line_index, start, end);
                    diagnostics.push(Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::WARNING),
                        source: Some("datastar".to_string()),
                        message: format!(
                            "Unknown action: '@{name}' is not a recognized Datastar action."
                        ),
                        ..Default::default()
                    });
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
}

fn check_value_signals(
    attr: &crate::analysis::ts_util::AttrData,
    line_index: &LineIndex,
    diagnostics: &mut Vec<Diagnostic>,
    defined: &std::collections::BTreeSet<String>,
) {
    emit_undefined_signals(attr, line_index, diagnostics, defined, None);
}

/// Scan a value for `$signal` references and emit diagnostics for undefined signals.
/// If `project_index` is Some, also checks cross-file definitions.
fn emit_undefined_signals(
    attr: &crate::analysis::ts_util::AttrData,
    line_index: &LineIndex,
    diagnostics: &mut Vec<Diagnostic>,
    defined: &std::collections::BTreeSet<String>,
    project_index: Option<&crate::analysis::project_index::ProjectIndex>,
) {
    let value = match &attr.value {
        Some(v) => v,
        None => return,
    };
    for signal in signal_util::scan_signals(value) {
        let top = signal.split('.').next().unwrap_or("");
        if signal_util::is_builtin_signal(top) {
            continue;
        }
        if defined.contains(top) {
            continue;
        }
        if let Some(index) = project_index {
            if !signal_util::index_find_def(index, top) {
                emit_undefined(attr, value, line_index, diagnostics, &signal);
            }
        } else {
            emit_undefined(attr, value, line_index, diagnostics, &signal);
        }
    }
}

fn emit_undefined(
    attr: &crate::analysis::ts_util::AttrData,
    value: &str,
    line_index: &LineIndex,
    diagnostics: &mut Vec<Diagnostic>,
    signal: &str,
) {
    if let (Some(value_start), Some(pos)) = (attr.value_start, value.find(&format!("${signal}"))) {
        let start = value_start + pos;
        let end = start + 1 + signal.len();
        let range = byte_range_to_lsp_range(line_index, start, end);
        diagnostics.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::HINT),
            source: Some("datastar".to_string()),
            message: format!("Undefined signal: '${signal}' is not defined in this document."),
            ..Default::default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diags_for(html: &str) -> Vec<Diagnostic> {
        let uri = Url::parse("file:///test.html").unwrap();
        let li = LineIndex::new(html.to_string());
        generate(&li, html, &uri, None)
    }

    #[test]
    fn test_unknown_attribute() {
        let diags = diags_for(r#"<div data-fake-thing="x"></div>"#);
        assert!(diags
            .iter()
            .any(|d| d.message.contains("Unknown Datastar attribute")));
    }

    #[test]
    fn test_missing_value() {
        let diags = diags_for(r#"<div data-on:click></div>"#);
        assert!(diags.iter().any(|d| d.message.contains("Missing value")));
    }

    #[test]
    fn test_undefined_signal() {
        let diags = diags_for(r#"<div data-text="$undefined"></div>"#);
        assert!(diags.iter().any(|d| d.message.contains("Undefined signal")));
    }

    #[test]
    fn test_valid_clean() {
        let diags = diags_for(r#"<div data-signals:foo="1" data-text="$foo"></div>"#);
        assert!(diags.is_empty());
    }
}
