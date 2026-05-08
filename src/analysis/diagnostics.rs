use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::analysis::project_index::ProjectIndex;
use crate::data::{actions, attributes, modifiers};
use crate::parser::html::{self, DataAttribute};

/// Generate diagnostics for a document. Optionally checks cross-file
/// index to suppress "undefined signal" hints.
pub fn generate(text: &str, project_index: Option<&ProjectIndex>) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Parse for data-* attributes. Try both HTML and JSX parsers.
    // JSX parser handles jsx_expression values properly; HTML sends them as stripped.
    // Use the parse that finds more attributes with values.
    let html_attrs: Vec<_> = html::parse_html(text.as_bytes())
        .map(|(_, a)| a)
        .unwrap_or_default();
    let jsx_attrs: Vec<_> = html::parse_jsx(text.as_bytes())
        .map(|(_, a)| a)
        .unwrap_or_default();

    // Pick the parser that found more attributes with actual values
    let html_with_vals = html_attrs.iter().filter(|a| a.value.is_some()).count();
    let jsx_with_vals = jsx_attrs.iter().filter(|a| a.value.is_some()).count();
    let data_attrs = if jsx_with_vals > html_with_vals || jsx_attrs.len() > html_attrs.len() {
        jsx_attrs
    } else {
        html_attrs
    };

    let attr_registry = attributes::all();
    let action_registry = actions::all();
    let modifier_registry = modifiers::all();

    // Per-attribute diagnostics
    for attr in &data_attrs {
        check_attribute_validity(
            attr,
            &attr_registry,
            &modifier_registry,
            text,
            &mut diagnostics,
        );
        check_expression_actions(attr, &action_registry, text, &mut diagnostics);
    }

    // Signal reference diagnostics
    let signal_analysis = crate::analysis::signals::analyze_signals(text);
    check_undefined_signals(&signal_analysis, text, &mut diagnostics, project_index);

    diagnostics
}

fn check_attribute_validity(
    attr: &DataAttribute,
    registry: &std::collections::BTreeMap<&str, attributes::AttributeDef>,
    modifier_registry: &std::collections::BTreeMap<&str, modifiers::ModifierDef>,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let def = match registry.get(attr.plugin_name.as_str()) {
        Some(d) => d,
        None => {
            // Unknown plugin name
            let range = crate::util::byte_range_to_lsp_range(
                text,
                attr.name_start,
                attr.name_start + attr.raw_name.len(),
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

    // Check key requirement
    match (def.key_req, &attr.key) {
        (attributes::Requirement::Must, None) => {
            let range = crate::util::byte_range_to_lsp_range(
                text,
                attr.name_start,
                attr.name_start + attr.raw_name.len(),
            );
            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("datastar".to_string()),
                message: format!(
                    "Missing key: 'data-{}' requires a key (e.g. 'data-{}:key').",
                    attr.plugin_name, attr.plugin_name
                ),
                ..Default::default()
            });
        }
        (attributes::Requirement::Denied, Some(key)) => {
            let key_pos = attr.raw_name.find(':').unwrap_or(0);
            let range = crate::util::byte_range_to_lsp_range(
                text,
                attr.name_start + key_pos,
                attr.name_start + key_pos + 1 + key.len(),
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
            let range = crate::util::byte_range_to_lsp_range(
                text,
                attr.name_start,
                attr.name_start + attr.raw_name.len(),
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

    // Check value requirement
    match (def.value_req, &attr.value) {
        (attributes::Requirement::Must, None) => {
            let range = crate::util::byte_range_to_lsp_range(
                text,
                attr.name_start,
                attr.name_start + attr.raw_name.len(),
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
            let range = crate::util::byte_range_to_lsp_range(
                text,
                attr.value_start.unwrap_or(attr.name_start),
                attr.value_start
                    .map(|s| s + attr.value.as_ref().map(|v| v.len() + 2).unwrap_or(0))
                    .unwrap_or(attr.name_start + attr.raw_name.len()),
            );
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

    // Check modifier keys against known modifiers for this attribute
    for (mod_key, _tags) in &attr.modifiers {
        // Validate the modifier key is known
        if !modifier_registry.contains_key(mod_key.as_str()) {
            let mod_pos = attr.raw_name.find(&format!("__{}", mod_key));
            if let Some(pos) = mod_pos {
                let start = attr.name_start + pos;
                let end = start + 2 + mod_key.len();
                let range = crate::util::byte_range_to_lsp_range(text, start, end);
                diagnostics.push(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some("datastar".to_string()),
                    message: format!(
                        "Unknown modifier: '__{}' is not a recognized modifier.",
                        mod_key
                    ),
                    ..Default::default()
                });
            }
        }
        // Check if this modifier is valid for this specific attribute
        if !modifier_registry.contains_key(mod_key.as_str()) {
            continue;
        }
        if !def.modifier_keys.contains(&mod_key.as_str()) && !is_global_modifier(mod_key) {
            let mod_pos = attr.raw_name.find(&format!("__{}", mod_key));
            if let Some(pos) = mod_pos {
                let start = attr.name_start + pos;
                let end = start + 2 + mod_key.len();
                let range = crate::util::byte_range_to_lsp_range(text, start, end);
                diagnostics.push(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some("datastar".to_string()),
                    message: format!(
                        "Modifier '__{}' is not valid for 'data-{}'.",
                        mod_key, attr.plugin_name
                    ),
                    ..Default::default()
                });
            }
        }
    }
}

/// Check if a modifier is valid for all attributes (global modifiers).
fn is_global_modifier(key: &str) -> bool {
    matches!(key, "case" | "delay" | "viewtransition")
}

fn check_expression_actions(
    attr: &DataAttribute,
    action_registry: &std::collections::BTreeMap<&str, actions::ActionDef>,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let value = match &attr.value {
        Some(v) => v,
        None => return,
    };

    // Simple scan for @actionName patterns
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
                let action_name = std::str::from_utf8(&bytes[i + 1..j]).unwrap_or("");
                if !action_registry.contains_key(action_name) {
                    // Skip if it's a local action (custom component actions)
                    if !action_name.is_empty()
                        && action_name
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_uppercase())
                    {
                        // Could be a variable, skip
                    } else if !action_name.is_empty() {
                        let value_start = attr.value_start.unwrap_or(0);
                        let start = value_start + 1 + i; // +1 for opening quote
                        let end = start + (j - i);
                        let range = crate::util::byte_range_to_lsp_range(text, start, end);
                        diagnostics.push(Diagnostic {
                            range,
                            severity: Some(DiagnosticSeverity::WARNING),
                            source: Some("datastar".to_string()),
                            message: format!(
                                "Unknown action: '@{}' is not a recognized Datastar action.",
                                action_name
                            ),
                            ..Default::default()
                        });
                    }
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
}

fn check_undefined_signals(
    analysis: &crate::analysis::signals::SignalAnalysis,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
    project_index: Option<&ProjectIndex>,
) {
    for ref_ in &analysis.references {
        let top_name = ref_.name.split('.').next().unwrap_or("");
        // Skip $evt, $el — built-in
        if top_name == "evt" || top_name == "el" {
            continue;
        }
        // Skip double-underscore signals (local component signals)
        if top_name.starts_with("__") {
            continue;
        }

        // Skip if defined locally
        if analysis.top_level_names.contains(top_name) {
            continue;
        }

        // Skip if defined in another open file (cross-file)
        if let Some(index) = project_index {
            if !index.find_definitions(top_name, None).is_empty() {
                continue;
            }
        }

        let range = crate::util::byte_range_to_lsp_range(
            text,
            ref_.byte_offset,
            ref_.byte_offset + 1 + ref_.name.len(),
        );
        diagnostics.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::HINT),
            source: Some("datastar".to_string()),
            message: format!(
                "Undefined signal: '${}' is not defined in any open file.",
                ref_.name
            ),
            ..Default::default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unknown_attribute() {
        let diags = generate(r#"<div data-fake-thing="x"></div>"#, None);
        assert!(diags
            .iter()
            .any(|d| d.message.contains("Unknown Datastar attribute")));
    }

    #[test]
    fn test_missing_value() {
        let diags = generate(r#"<div data-on:click></div>"#, None);
        assert!(diags.iter().any(|d| d.message.contains("Missing value")));
    }

    #[test]
    fn test_undefined_signal() {
        let diags = generate(r#"<div data-text="$undefined"></div>"#, None);
        assert!(diags.iter().any(|d| d.message.contains("Undefined signal")));
    }

    #[test]
    fn test_valid_clean() {
        let diags = generate(r#"<div data-signals:foo="1" data-text="$foo"></div>"#, None);
        assert!(diags.is_empty());
    }
}
