use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, Position, Url,
};

use crate::analysis::cursor::{self, CursorPosition};
use crate::analysis::signal_util::{DEFINERS, GLOBAL_MODIFIERS};
use crate::analysis::ts_util::{self, AttrData};
use crate::data::{actions, attributes};
use crate::line_index::LineIndex;

pub fn generate(
    line_index: &LineIndex,
    text: &str,
    position: Position,
    uri: &Url,
) -> Vec<CompletionItem> {
    let cursor_byte = line_index.position_to_byte_offset(position.line, position.character);
    let mut items = Vec::new();

    let Some((tree, attrs)) = ts_util::parse_and_collect(text, uri) else {
        return vec![];
    };

    let ctx = cursor::detect(tree.root_node(), text, cursor_byte);

    match ctx {
        CursorPosition::AttributeName { plugin_name: _ } => {
            // Cursor is inside a data-* attribute name — complete attribute names
            items.extend(complete_attribute_names(&attrs));
        }
        CursorPosition::AfterColon {
            plugin_name,
            key: _,
        } => {
            // Cursor is after the colon in data-plugin:key__modifier
            // Complete modifiers for the plugin
            if let Some(def) = attributes::all().get(plugin_name.as_str()) {
                // Find the attr data to get used modifiers
                let used_mods = attrs
                    .iter()
                    .find(|a| {
                        a.plugin_name == plugin_name
                            && a.name_start <= cursor_byte
                            && a.name_start + a.name_len >= cursor_byte
                    })
                    .map(|a| &a.modifiers);
                items.extend(complete_modifiers(def, used_mods.unwrap_or(&Vec::new())));
            }
        }
        CursorPosition::AttributeValue {
            value_start,
            full_value,
            ..
        } => {
            // Cursor is inside a data-* attribute value
            value_completions(&mut items, &attrs, cursor_byte, value_start, &full_value);
        }
        CursorPosition::InMarkup => {
            // Cursor is in markup but not inside a data-* attr — offer attribute names
            items.extend(complete_attribute_names(&attrs));
        }
        CursorPosition::None => {
            // Not in a Datastar context — no completions
        }
    }

    deduplicate_and_sort(items)
}

/// Determine value completions based on what precedes the cursor in the value.
fn value_completions(
    items: &mut Vec<CompletionItem>,
    attrs: &[AttrData],
    cursor_byte: usize,
    value_start: usize,
    full_value: &str,
) {
    if cursor_byte <= value_start {
        // Cursor at or before value start — show based on first char
        if full_value.starts_with('@') {
            items.extend(complete_actions());
        } else if full_value.starts_with('$') {
            items.extend(complete_signals(attrs));
        }
        return;
    }

    let rel = cursor_byte.saturating_sub(value_start);
    if rel == 0 || rel > full_value.len() {
        return;
    }

    let before = &full_value[..rel.min(full_value.len())];

    if before.ends_with('$') || before.ends_with("$.") {
        items.extend(complete_signals(attrs));
    } else if before.ends_with('@') {
        items.extend(complete_actions());
    } else if let Some(pos) = before.rfind('$') {
        let after_dollar = &before[pos + 1..];
        if !after_dollar.contains(' ') && !after_dollar.contains('"') {
            let prefix = if after_dollar.contains('.') {
                after_dollar.split('.').next_back()
            } else {
                Some(after_dollar)
            };
            items.extend(complete_signals_filtered(attrs, prefix));
        }
    } else if let Some(pos) = before.rfind('@') {
        let after = &before[pos + 1..];
        if !after.contains(' ') && !after.contains('(') {
            items.extend(complete_actions());
        }
    }
}

fn complete_attribute_names(existing: &[AttrData]) -> Vec<CompletionItem> {
    let registry = attributes::all();
    let used: std::collections::BTreeSet<&str> =
        existing.iter().map(|a| a.plugin_name.as_str()).collect();

    registry
        .iter()
        .filter(|(name, _)| !used.contains(*name))
        .map(|(name, def)| {
            let label = format!("data-{name}");
            let insert = match def.key_req {
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
            CompletionItem {
                label,
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(if def.pro { "Datastar Pro" } else { "Datastar" }.to_string()),
                documentation: Some(Documentation::String(format!(
                    "{}\n\n[Documentation]({})",
                    def.description, def.doc_url
                ))),
                insert_text: Some(insert),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            }
        })
        .collect()
}

fn complete_actions() -> Vec<CompletionItem> {
    actions::all()
        .iter()
        .map(|(name, def)| {
            let params = def.params.join(", ");
            CompletionItem {
                label: format!("@{name}"),
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
                    "{}\n\n`@{name}`({params})\n\n[Documentation]({})",
                    def.description, def.doc_url
                ))),
                insert_text: Some(format!("{name}({params})")),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            }
        })
        .collect()
}

fn complete_signals(attrs: &[AttrData]) -> Vec<CompletionItem> {
    complete_signals_filtered(attrs, None)
}

fn complete_signals_filtered(attrs: &[AttrData], prefix: Option<&str>) -> Vec<CompletionItem> {
    let mut seen = std::collections::BTreeSet::new();
    attrs
        .iter()
        .filter(|a| DEFINERS.contains(&a.plugin_name.as_str()))
        .filter_map(|a| a.key.as_ref())
        .filter(|k| seen.insert(*k))
        .filter(|n| prefix.is_none_or(|p| n.starts_with(p) && n.len() > p.len()))
        .map(|name| CompletionItem {
            label: format!("${name}"),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("signal".to_string()),
            insert_text: Some(name.clone()),
            ..Default::default()
        })
        .collect()
}

fn complete_modifiers(
    def: &attributes::AttributeDef,
    used_modifiers: &[(String, Vec<String>)],
) -> Vec<CompletionItem> {
    let registry = crate::data::modifiers::all();
    let used: std::collections::BTreeSet<&str> =
        used_modifiers.iter().map(|(k, _)| k.as_str()).collect();

    let mut items: Vec<CompletionItem> = Vec::new();

    for mod_key in def.modifier_keys {
        if used.contains(mod_key) {
            continue;
        }
        if let Some(mod_def) = registry.get(mod_key) {
            let insert =
                if mod_def.tags.is_empty() || mod_def.tags == crate::data::modifiers::ANY_TAG {
                    format!("__{mod_key}")
                } else {
                    format!("__{mod_key}.{}", mod_def.tags[0])
                };
            items.push(CompletionItem {
                label: format!("__{mod_key}"),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some("modifier".to_string()),
                documentation: Some(Documentation::String(mod_def.description.to_string())),
                insert_text: Some(insert),
                ..Default::default()
            });
        }
    }

    for global in GLOBAL_MODIFIERS {
        if used.contains(global) {
            continue;
        }
        if let Some(mod_def) = registry.get(global) {
            let insert = if *global == "delay" {
                "__delay.500ms".to_string()
            } else {
                format!("__{global}")
            };
            items.push(CompletionItem {
                label: format!("__{global}"),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some("modifier (global)".to_string()),
                documentation: Some(Documentation::String(mod_def.description.to_string())),
                insert_text: Some(insert),
                ..Default::default()
            });
        }
    }

    items
}

fn deduplicate_and_sort(items: Vec<CompletionItem>) -> Vec<CompletionItem> {
    let mut seen = std::collections::BTreeSet::new();
    let mut result = Vec::new();
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
    fn test_complete_actions_direct() {
        let items = complete_actions();
        assert!(items.iter().any(|a| a.label == "@get"));
        assert!(items.iter().any(|a| a.label == "@post"));
        assert!(items.iter().any(|a| a.label == "@peek"));
    }

    #[test]
    fn test_complete_signals_direct() {
        let html = r#"<div data-signals:foo="1" data-bind:bar><span data-text="$"></span></div>"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_html::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(html, None).unwrap();
        let attrs = crate::analysis::ts_util::collect_from_tree(tree.root_node(), html);
        let items = complete_signals(&attrs);
        assert!(items.iter().any(|s| s.label == "$foo"));
        assert!(items.iter().any(|s| s.label == "$bar"));
    }

    #[test]
    fn test_complete_at_action() {
        let html = r#"<div data-signals:counter="0"></div>
<div data-show="$counter > 0">
  <button data-on:click="@get('/api/data')">Load</button>
</div>"#;
        let line_index = LineIndex::new(html.to_string());
        let at_byte = html.find("@get").unwrap();
        let (l, c) = line_index.byte_to_position(at_byte);
        let uri = Url::parse("file:///test.html").unwrap();
        let items = generate(
            &line_index,
            html,
            Position {
                line: l,
                character: c,
            },
            &uri,
        );
        assert!(
            items.iter().any(|i| i.label == "@get"),
            "should suggest @get, got: {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
    }
}
