use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Url};

use crate::analysis::signal_util::{self, DEFINERS};
use crate::analysis::ts_util;
use crate::data::{actions, attributes, modifiers};
use crate::line_index::LineIndex;
use crate::util::byte_range_to_lsp_range;

/// Common HTML DOM events used with `data-on:`.
pub const KNOWN_DOM_EVENTS: &[&str] = &[
    // Mouse events
    "click",
    "dblclick",
    "contextmenu",
    "mousedown",
    "mouseup",
    "mousemove",
    "mouseenter",
    "mouseleave",
    "mouseover",
    "mouseout",
    "wheel",
    // Keyboard events
    "keydown",
    "keyup",
    "keypress",
    // Form events
    "focus",
    "blur",
    "change",
    "input",
    "submit",
    "reset",
    "select",
    // Window/document events
    "load",
    "unload",
    "beforeunload",
    "scroll",
    "resize",
    "error",
    // Touch events
    "touchstart",
    "touchend",
    "touchmove",
    "touchcancel",
    // Pointer events
    "pointerdown",
    "pointerup",
    "pointermove",
    "pointerenter",
    "pointerleave",
    "pointercancel",
    // Drag events
    "drag",
    "dragstart",
    "dragend",
    "dragenter",
    "dragleave",
    "dragover",
    "drop",
    // Clipboard events
    "copy",
    "cut",
    "paste",
    // Media events
    "play",
    "pause",
    "ended",
    "volumechange",
    "timeupdate",
    // Animation/transition
    "animationend",
    "animationstart",
    "transitionend",
    // Datastar custom events
    "datastar-fetch",
    "rocket-launched",
];

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
        .flat_map(signal_util::signal_names_from_attr)
        .collect();

    for attr in &attrs {
        check_attribute_validity(
            attr,
            &attr_registry,
            &modifier_registry,
            line_index,
            &mut diagnostics,
        );
        check_modifier_conflicts(attr, line_index, &mut diagnostics);
        check_expression_syntax(attr, line_index, &mut diagnostics);
        check_value_actions(attr, &action_registry, line_index, &mut diagnostics);
    }

    // Signal checks: emit once (either local-only or with cross-file fallback)
    if let Some(index) = project_index {
        for attr in &attrs {
            emit_undefined_signals(
                attr,
                line_index,
                &mut diagnostics,
                &defined_signals,
                Some(index),
            );
        }
    } else {
        for attr in &attrs {
            emit_undefined_signals(attr, line_index, &mut diagnostics, &defined_signals, None);
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
            // Plain HTML data-* attributes (no colon, no __modifiers, no Datastar value)
            // are legitimate HTML — don't flag them.
            let has_colon = attr.raw_name.contains(':');
            let has_modifiers = !attr.modifiers.is_empty();
            let has_datastar_value = attr
                .value
                .as_ref()
                .is_some_and(|v| v.contains('$') || v.contains('@'));
            if !has_colon && !has_modifiers && !has_datastar_value {
                return;
            }
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
        (attributes::Requirement::Must, Some(key))
            if attr.plugin_name == "on" && !KNOWN_DOM_EVENTS.contains(&key.as_str()) =>
        {
            let pos = attr.raw_name.find(':').unwrap_or(0);
            let range = byte_range_to_lsp_range(
                line_index,
                attr.name_start + pos + 1,
                attr.name_start + pos + 1 + key.len(),
            );
            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::WARNING),
                source: Some("datastar".to_string()),
                message: format!("Unknown event: '{key}' is not a recognized event name.",),
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
            push_modifier_diag(
                diagnostics,
                attr,
                line_index,
                mod_key,
                &format!("Unknown modifier: '__{mod_key}' is not a recognized modifier."),
            );
            continue;
        }
        if !def.modifier_keys.contains(&mod_key.as_str())
            && !signal_util::is_global_modifier(mod_key)
        {
            push_modifier_diag(
                diagnostics,
                attr,
                line_index,
                mod_key,
                &format!(
                    "Modifier '__{mod_key}' is not valid for 'data-{}'.",
                    attr.plugin_name
                ),
            );
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
    for span in crate::analysis::value_scanner::scan_value(value) {
        if span.kind != crate::analysis::value_scanner::SpanKind::AtAction {
            continue;
        }
        if registry.contains_key(span.name.as_str())
            || span.name.starts_with(|c: char| c.is_ascii_uppercase())
        {
            continue;
        }
        let vs = attr.value_start.unwrap_or(0);
        let start = vs + span.start;
        let end = vs + span.end;
        let range = byte_range_to_lsp_range(line_index, start, end);
        diagnostics.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::WARNING),
            source: Some("datastar".to_string()),
            message: format!(
                "Unknown action: '@{}' is not a recognized Datastar action.",
                span.name
            ),
            ..Default::default()
        });
    }
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
    for span in crate::analysis::value_scanner::scan_value(value) {
        if span.kind != crate::analysis::value_scanner::SpanKind::DollarSignal {
            continue;
        }
        let top = span.name.split('.').next().unwrap_or("");
        // Also strip trailing method call for diag message only
        let display = span.name.trim_end_matches(|c: char| c == '(' || c == ')');
        if signal_util::is_builtin_signal(top) {
            continue;
        }
        if defined.contains(top) {
            continue;
        }
        if let Some(index) = project_index {
            if !signal_util::index_find_def(index, top) {
                emit_undefined(attr, value, line_index, diagnostics, display);
            }
        } else {
            emit_undefined(attr, value, line_index, diagnostics, display);
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
    if let Some(value_start) = attr.value_start {
        if let Some(pos) = value.find(&format!("${signal}")) {
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
}

fn push_modifier_diag(
    diagnostics: &mut Vec<Diagnostic>,
    attr: &crate::analysis::ts_util::AttrData,
    line_index: &LineIndex,
    mod_key: &str,
    message: &str,
) {
    if let Some(pos) = attr.raw_name.find(&format!("__{mod_key}")) {
        let start = attr.name_start + pos;
        let end = start + 2 + mod_key.len();
        let range = byte_range_to_lsp_range(line_index, start, end);
        diagnostics.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::WARNING),
            source: Some("datastar".to_string()),
            message: message.to_string(),
            ..Default::default()
        });
    }
}

/// Conflicts between modifier groups (mutually exclusive).
/// These are defined per-plugin: modifiers within a group cannot be used together.
const MODIFIER_CONFLICTS: &[(&str, &[&[&str]])] = &[(
    "scroll-into-view",
    &[
        &["smooth", "instant", "auto"],
        &["hstart", "hcenter", "hend", "hnearest"],
        &["vstart", "vcenter", "vend", "vnearest"],
    ],
)];

/// Check for duplicate and conflicting modifiers on an attribute.
fn check_modifier_conflicts(
    attr: &crate::analysis::ts_util::AttrData,
    line_index: &LineIndex,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Duplicate modifiers
    let mut seen = std::collections::BTreeMap::new();
    for (mod_key, _tags) in &attr.modifiers {
        if let Some(prev_pos) = seen.get(mod_key.as_str()) {
            let range = byte_range_to_lsp_range(
                line_index,
                attr.name_start + prev_pos,
                attr.name_start + prev_pos + 2 + mod_key.len(),
            );
            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::WARNING),
                source: Some("datastar".to_string()),
                message: format!("Duplicate modifier: '__{mod_key}' appears more than once."),
                ..Default::default()
            });
        }
        if let Some(pos) = attr.raw_name.find(&format!("__{mod_key}")) {
            seen.insert(mod_key.as_str(), pos);
        }
    }

    // Conflicting modifier groups
    for (plugin, groups) in MODIFIER_CONFLICTS {
        if attr.plugin_name != *plugin {
            continue;
        }
        for group in *groups {
            let found: Vec<&str> = attr
                .modifiers
                .iter()
                .filter(|(k, _)| group.contains(&k.as_str()))
                .map(|(k, _)| k.as_str())
                .collect();
            if found.len() > 1 {
                let first = found[0];
                if let Some(pos) = attr.raw_name.find(&format!("__{first}")) {
                    let range = byte_range_to_lsp_range(
                        line_index,
                        attr.name_start + pos,
                        attr.name_start + pos + 2 + first.len(),
                    );
                    diagnostics.push(Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::WARNING),
                        source: Some("datastar".to_string()),
                        message: format!("Conflicting modifiers: {} (pick one).", found.join(", ")),
                        ..Default::default()
                    });
                }
            }
        }
    }
}

/// Check expression syntax: balanced delimiters, unterminated strings.
fn check_expression_syntax(
    attr: &crate::analysis::ts_util::AttrData,
    line_index: &LineIndex,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let value = match &attr.value {
        Some(v) => v,
        None => return,
    };
    if value.trim().is_empty() {
        return;
    }

    let bytes = value.as_bytes();
    let mut stack: Vec<(u8, usize)> = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i];

        // String literals
        if c == b'"' || c == b'\'' || c == b'`' {
            let quote = c;
            i += 1;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                } else if bytes[i] == quote {
                    i += 1;
                    closed = true;
                    break;
                } else {
                    i += 1;
                }
            }
            if !closed {
                let vs = attr.value_start.unwrap_or(0);
                let range = byte_range_to_lsp_range(line_index, vs, vs + value.len());
                diagnostics.push(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("datastar".to_string()),
                    message: format!("Unterminated string (opened with '{}').", quote as char),
                    ..Default::default()
                });
                return;
            }
            continue;
        }

        // Opening delimiters
        if c == b'(' || c == b'[' || c == b'{' {
            stack.push((c, i));
            i += 1;
            continue;
        }

        // Closing delimiters
        let expected = match c {
            b')' => b'(',
            b']' => b'[',
            b'}' => b'{',
            _ => 0,
        };
        if expected != 0 {
            if stack.is_empty() || stack.last().unwrap().0 != expected {
                let vs = attr.value_start.unwrap_or(0);
                let pos = vs + i;
                let range = byte_range_to_lsp_range(line_index, pos, pos + 1);
                diagnostics.push(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("datastar".to_string()),
                    message: format!(
                        "Unexpected '{}' without matching '{}'.",
                        c as char, expected as char
                    ),
                    ..Default::default()
                });
                return;
            }
            stack.pop();
        }

        i += 1;
    }

    // Unclosed delimiters
    if let Some((open, _)) = stack.last() {
        let close = match open {
            b'(' => b')',
            b'[' => b']',
            b'{' => b'}',
            _ => b'?',
        };
        let vs = attr.value_start.unwrap_or(0);
        let range = byte_range_to_lsp_range(line_index, vs, vs + value.len());
        diagnostics.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("datastar".to_string()),
            message: format!(
                "Unclosed '{}' — expected matching '{}'.",
                *open as char, close as char
            ),
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
        let diags = diags_for(r#"<div data-fake:thing="x"></div>"#);
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

    #[test]
    fn test_duplicate_modifier() {
        let html = r#"<div data-on:click__debounce__debounce="$x"></div>"#;
        let diags = diags_for(html);
        assert!(diags
            .iter()
            .any(|d| d.message.contains("Duplicate modifier")));
    }

    #[test]
    fn test_conflicting_modifiers() {
        let html = r#"<div data-scroll-into-view__smooth__instant></div>"#;
        let diags = diags_for(html);
        assert!(diags
            .iter()
            .any(|d| d.message.contains("Conflicting modifiers")));
    }

    #[test]
    fn test_unclosed_paren() {
        let html = r#"<div data-text="$foo + ($bar"></div>"#;
        let diags = diags_for(html);
        assert!(diags.iter().any(|d| d.message.contains("Unclosed")));
    }

    #[test]
    fn test_unterminated_string() {
        let html = r#"<div data-text='$foo + "hello'></div>"#;
        let diags = diags_for(html);
        assert!(diags
            .iter()
            .any(|d| d.message.contains("Unterminated string")));
    }

    #[test]
    fn test_balanced_expression_clean() {
        let html = r#"<div data-text="$foo + ($bar ? 'yes' : 'no')"></div>"#;
        let diags = diags_for(html);
        assert!(!diags.iter().any(|d| d.message.contains("Unclosed")
            || d.message.contains("Unterminated")
            || d.message.contains("Unexpected")));
    }

    #[test]
    fn test_bind_value_defines_signal() {
        let html = r#"<input data-bind="percentage" /><button data-on:click="$percentage = 50">Set</button>"#;
        let diags = diags_for(html);
        assert!(!diags
            .iter()
            .any(|d| d.message.contains("Undefined signal: '$percentage'")));
    }

    #[test]
    fn test_signals_object_defines_signals() {
        let html = r#"<div data-signals="{percentage: 0, contents: 'hello'}" data-effect="$percentage = $contents.toUpperCase()"></div>"#;
        let diags = diags_for(html);
        assert!(!diags
            .iter()
            .any(|d| d.message.contains("Undefined signal: '$percentage'")));
        assert!(!diags
            .iter()
            .any(|d| d.message.contains("Undefined signal: '$contents'")));
    }
}
