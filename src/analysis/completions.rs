use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, Position,
};

use crate::analysis::signals::SignalAnalysis;
use crate::data::{actions, attributes};
use crate::line_index::LineIndex;
use crate::parser::html::DataAttribute;

/// Context for generating completions at a cursor position
pub struct CompletionContext<'a> {
    /// Document line index (has text + position conversion)
    pub line_index: &'a LineIndex,
    /// Cursor position (line, character)
    pub position: Position,
    /// Parsed data attributes from the document
    pub data_attrs: Vec<DataAttribute>,
    /// Pre-computed signal analysis
    pub signal_analysis: &'a SignalAnalysis,
}

/// Generate completion items for the given context.
pub fn generate(ctx: &CompletionContext) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    let cursor_offset = ctx
        .line_index
        .position_to_byte_offset(ctx.position.line, ctx.position.character);

    if let Some(attr) = find_attribute_at_offset(&ctx.data_attrs, cursor_offset) {
        let attr_registry = attributes::all();

        // Cursor is in attribute name → suggest known attribute names
        if cursor_offset >= attr.name_start
            && cursor_offset <= attr.name_start + attr.raw_name.len()
        {
            // Check if cursor is after ":" — suggest modifier completions
            if let Some(colon_pos) = attr.raw_name.find(':') {
                let colon_offset = attr.name_start + colon_pos;
                if cursor_offset > colon_offset {
                    // Inside key or modifier → suggest known modifier keys
                    if let Some(def) = attr_registry.get(attr.plugin_name.as_str()) {
                        items.extend(complete_modifiers(def, attr, colon_offset, &attr_registry));
                    }
                    return deduplicate_and_sort(items);
                }
            }

            // Suggest attribute names
            items.extend(complete_attribute_names(
                cursor_offset,
                &attr_registry,
                &ctx.data_attrs,
            ));
            return deduplicate_and_sort(items);
        }

        // Cursor is in attribute value → suggest signals, actions
        if let (Some(value_start), Some(value)) = (attr.value_start, &attr.value) {
            let value_end = value_start + value.len() + 2; // +2 for quotes
            if cursor_offset >= value_start && cursor_offset <= value_end {
                let relative_offset = cursor_offset.saturating_sub(value_start + 1); // past opening quote

                // If cursor is after $, suggest defined signals
                if relative_offset > 0 && relative_offset <= value.len() {
                    let before_cursor = &value[..relative_offset.min(value.len())];
                    if before_cursor.ends_with('$') || before_cursor.ends_with("$.") {
                        items.extend(complete_signals(ctx.signal_analysis, &ctx.data_attrs));
                    } else if before_cursor.ends_with('@') {
                        items.extend(complete_actions(&actions::all()));
                    } else if let Some(last_dollar) = before_cursor.rfind('$') {
                        // Cursor is inside a signal name like $foo|.bar
                        let after_dollar = &before_cursor[last_dollar + 1..];
                        if !after_dollar.contains(' ') && !after_dollar.contains('"') {
                            items.extend(complete_signals(ctx.signal_analysis, &ctx.data_attrs));
                        }
                    } else if let Some(last_at) = before_cursor.rfind('@') {
                        let after_at = &before_cursor[last_at + 1..];
                        if !after_at.contains(' ') && !after_at.contains('(') {
                            items.extend(complete_actions(&actions::all()));
                        }
                    }
                }
                // Cursor right after opening quote — value starts with @ or $
                if relative_offset == 0 {
                    if value.starts_with('@') {
                        items.extend(complete_actions(&actions::all()));
                    } else if value.starts_with('$') {
                        items.extend(complete_signals(ctx.signal_analysis, &ctx.data_attrs));
                    }
                }
                return deduplicate_and_sort(items);
            }
        }
    }

    // Default: suggest all data- attributes
    let attr_registry = attributes::all();
    items.extend(complete_attribute_names(
        cursor_offset,
        &attr_registry,
        &ctx.data_attrs,
    ));
    deduplicate_and_sort(items)
}

/// Complete known data-* attribute names.
fn complete_attribute_names(
    cursor_offset: usize,
    registry: &std::collections::BTreeMap<&str, attributes::AttributeDef>,
    existing: &[DataAttribute],
) -> Vec<CompletionItem> {
    // Filter out already-used exclusive attributes
    let used: std::collections::BTreeSet<&str> = existing
        .iter()
        .filter(|a| a.name_start < cursor_offset)
        .map(|a| a.plugin_name.as_str())
        .collect();

    registry
        .iter()
        .filter(|(name, _)| !used.contains(*name))
        .map(|(name, def)| {
            let label = format!("data-{}", name);
            let insert_text = match def.key_req {
                attributes::Requirement::Must => {
                    format!("data-{name}:${{1:key}}=\"${{2:expression}}\"")
                }
                attributes::Requirement::Allowed | attributes::Requirement::Exclusive => {
                    format!("data-{name}=\"${{1:expression}}\"")
                }
                attributes::Requirement::Denied => {
                    if def.value_req == attributes::Requirement::Must {
                        format!("data-{name}=\"${{1:expression}}\"")
                    } else {
                        format!("data-{name}")
                    }
                }
            };

            let kind = CompletionItemKind::PROPERTY;
            let _pro = def.pro;

            CompletionItem {
                label,
                kind: Some(kind),
                detail: Some(if def.pro { "Datastar Pro" } else { "Datastar" }.to_string()),
                documentation: Some(Documentation::String(format!(
                    "{}\n\n[Documentation]({})",
                    def.description, def.doc_url
                ))),
                insert_text: Some(insert_text),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            }
        })
        .collect()
}

/// Complete known action names (e.g. `@get`, `@post`).
fn complete_actions(
    action_registry: &std::collections::BTreeMap<&str, actions::ActionDef>,
) -> Vec<CompletionItem> {
    action_registry
        .iter()
        .map(|(name, def)| {
            let params = def.params.join(", ");
            let insert_text = format!("{name}({params})");

            CompletionItem {
                label: format!("@{}", name),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(
                    if def.pro {
                        "Datastar Pro action"
                    } else {
                        "Datastar action"
                    }
                    .to_string(),
                ),
                documentation: Some(Documentation::String(format!(
                    "{}\n\n`@{}`({})\n\n[Documentation]({})",
                    def.description, name, params, def.doc_url
                ))),
                insert_text: Some(insert_text),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            }
        })
        .collect()
}

/// Complete signal names defined in the current document.
fn complete_signals(
    analysis: &SignalAnalysis,
    attributes: &[DataAttribute],
) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = Vec::new();

    for (name, defs) in &analysis.definitions {
        let def_by = defs.first().map(|d| d.defined_by.as_str()).unwrap_or("");
        let detail = format!("signal (defined via data-{})", def_by);

        items.push(CompletionItem {
            label: format!("${}", name),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some(detail),
            insert_text: Some(name.clone()),
            ..Default::default()
        });
    }

    // Also suggest unused signal-defining attribute keys for completion in attribute names
    // e.g. data-bind: — suggest if there's already data-signals:* in scope
    for attr in attributes {
        if attr.plugin_name == "signals" {
            if let Some(key) = &attr.key {
                if !analysis.definitions.contains_key(key) {
                    items.push(CompletionItem {
                        label: format!("${}", key),
                        kind: Some(CompletionItemKind::VARIABLE),
                        detail: Some("signal (from data-signals)".to_string()),
                        insert_text: Some(key.clone()),
                        ..Default::default()
                    });
                }
            }
        }
    }

    items
}

/// Complete modifier keys for a specific attribute.
fn complete_modifiers(
    def: &attributes::AttributeDef,
    attr: &DataAttribute,
    _colon_offset: usize,
    _registry: &std::collections::BTreeMap<&str, attributes::AttributeDef>,
) -> Vec<CompletionItem> {
    let modifier_registry = crate::data::modifiers::all();

    // Determine which modifiers are already used on this attribute
    let used_mods: std::collections::BTreeSet<&str> =
        attr.modifiers.iter().map(|(k, _)| k.as_str()).collect();

    let mut items: Vec<CompletionItem> = Vec::new();

    for mod_key in def.modifier_keys {
        if used_mods.contains(mod_key) {
            continue;
        }
        if let Some(mod_def) = modifier_registry.get(mod_key) {
            let insert_text =
                if mod_def.tags.is_empty() || mod_def.tags == crate::data::modifiers::ANY_TAG {
                    format!("__{}", mod_key)
                } else {
                    let tag = mod_def.tags[0];
                    format!("__{}.{}", mod_key, tag)
                };

            items.push(CompletionItem {
                label: format!("__{}", mod_key),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some("modifier".to_string()),
                documentation: Some(Documentation::String(mod_def.description.to_string())),
                insert_text: Some(insert_text),
                ..Default::default()
            });
        }
    }

    // Always suggest global modifiers
    for global in &["case", "delay", "viewtransition"] {
        if used_mods.contains(global) {
            continue;
        }
        if let Some(mod_def) = modifier_registry.get(global) {
            let insert_text =
                if mod_def.tags.is_empty() || mod_def.tags == crate::data::modifiers::ANY_TAG {
                    if *global == "delay" {
                        "__delay.500ms".to_string()
                    } else {
                        format!("__{}", global)
                    }
                } else {
                    format!("__{}", global)
                };

            items.push(CompletionItem {
                label: format!("__{}", global),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some("modifier (global)".to_string()),
                documentation: Some(Documentation::String(mod_def.description.to_string())),
                insert_text: Some(insert_text),
                ..Default::default()
            });
        }
    }

    items
}

fn find_attribute_at_offset(attrs: &[DataAttribute], offset: usize) -> Option<&DataAttribute> {
    attrs.iter().find(|a| {
        offset >= a.name_start && offset <= a.name_start + a.raw_name.len()
            || a.value_start.is_some_and(|vs| {
                offset >= vs && offset <= vs + a.value.as_ref().map(|v| v.len()).unwrap_or(0) + 2
            })
    })
}

fn deduplicate_and_sort(items: Vec<CompletionItem>) -> Vec<CompletionItem> {
    let mut seen = std::collections::BTreeSet::new();
    let mut result: Vec<CompletionItem> = Vec::new();
    for item in items {
        if seen.insert(item.label.clone()) {
            result.push(item);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_actions() {
        let actions = complete_actions(&crate::data::actions::all());
        assert!(actions.iter().any(|a| a.label == "@get"));
        assert!(actions.iter().any(|a| a.label == "@post"));
        assert!(actions.iter().any(|a| a.label == "@peek"));
    }

    #[test]
    fn test_complete_signals() {
        let html = r#"<div data-signals:foo="1" data-bind:bar><span data-text="$"></span></div>"#;
        let parsed = crate::parser::html::parse_html(html.as_bytes()).unwrap();
        let analysis = crate::analysis::signals::analyze_signals(html);
        let signals = complete_signals(&analysis, &parsed.1);
        assert!(signals.iter().any(|s| s.label == "$foo"));
        assert!(signals.iter().any(|s| s.label == "$bar"));
    }
}

#[test]
fn test_complete_after_signals_block() {
    let html = r#"<div data-signals:counter="0"></div>
<div data-show="$counter > 0">
  <button data-on:click="@get('/api/data')">Load</button>
</div>"#;
    let (_, attrs) = crate::parser::html::parse_html(html.as_bytes()).unwrap();
    let analysis = crate::analysis::signals::analyze_signals(html);
    let line_index = crate::line_index::LineIndex::new(html.to_string());

    // Find @get in the button
    let at_byte = html.find("@get").unwrap();
    let (l, c) = line_index.byte_to_position(at_byte);
    eprintln!(
        "attrs: {:?}",
        attrs
            .iter()
            .map(|a| format!("{} v={:?} vs={:?}", a.raw_name, a.value, a.value_start))
            .collect::<Vec<_>>()
    );
    eprintln!("@get at byte={at_byte} pos=({l},{c})");

    let ctx = CompletionContext {
        line_index: &line_index,
        position: Position {
            line: l,
            character: c,
        },
        data_attrs: attrs.clone(),
        signal_analysis: &analysis,
    };
    let items = generate(&ctx);
    eprintln!("items: {}", items.len());
    for i in &items {
        eprintln!("  {}", i.label);
    }
    assert!(
        items.iter().any(|i| i.label == "@get"),
        "should suggest @get"
    );
}

#[test]
fn debug_name_and_value_ranges() {
    let html = r#"<div data-signals:counter="0"></div>
<div data-show="$counter > 0">
  <button data-on:click="@get('/api/data')">Load</button>
</div>"#;
    let (_, attrs) = crate::parser::html::parse_html(html.as_bytes()).unwrap();
    for a in &attrs {
        println!(
            "attr '{}': name_start={} name_end={} raw_len={} value_start={:?} value_len={}",
            a.raw_name,
            a.name_start,
            a.name_start + a.raw_name.len(),
            a.raw_name.len(),
            a.value_start,
            a.value.as_ref().map(|v| v.len()).unwrap_or(0)
        );
    }
    let at_byte = html.find("@get").unwrap();
    println!("\n@get at byte {}", at_byte);
}

#[test]
fn debug_find_attr() {
    let html = r#"<div data-signals:counter="0"></div>
<div data-show="$counter > 0">
  <button data-on:click="@get('/api/data')">Load</button>
</div>"#;
    let (_, attrs) = crate::parser::html::parse_html(html.as_bytes()).unwrap();
    let at_byte = html.find("@get").unwrap();
    println!("cursor at {}", at_byte);
    for a in &attrs {
        let name_end = a.name_start + a.raw_name.len();
        let val_end =
            a.value_start.unwrap_or(0) + a.value.as_ref().map(|v| v.len()).unwrap_or(0) + 2;
        let in_name = at_byte >= a.name_start && at_byte <= name_end;
        let in_val = a
            .value_start
            .is_some_and(|vs| at_byte >= vs && at_byte <= val_end);
        println!(
            "  '{}': ({}..{}) val_start={:?} val_end={} name={} val={}",
            a.raw_name, a.name_start, name_end, a.value_start, val_end, in_name, in_val
        );
    }
}

#[test]
fn debug_find_attr_result() {
    let html = r#"<div data-signals:counter="0"></div>
<div data-show="$counter > 0">
  <button data-on:click="@get('/api/data')">Load</button>
</div>"#;
    let (_, attrs) = crate::parser::html::parse_html(html.as_bytes()).unwrap();
    let at_byte = html.find("@get").unwrap();
    let found = find_attribute_at_offset(&attrs, at_byte);
    println!(
        "find_attribute_at_offset({}) = {:?}",
        at_byte,
        found.map(|a| &a.raw_name)
    );
}
