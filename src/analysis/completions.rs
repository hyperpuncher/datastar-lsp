use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, Position, Url,
};

use crate::analysis::cursor::{self, CursorPosition};
use crate::analysis::ts_util::{self, AttrData};
use crate::data::events::KNOWN_DOM_EVENTS;
use crate::data::{actions, attributes};
use crate::line_index::LineIndex;

use crate::analysis::signal_util::GLOBAL_MODIFIERS;

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
        CursorPosition::AfterColon { plugin_name, key }
        | CursorPosition::AttrsPropKey { plugin_name, key } => {
            if let Some(def) = attributes::all().get(plugin_name.as_str()) {
                let matching_attr = attrs.iter().find(|a| {
                    a.plugin_name == plugin_name
                        && a.name_start <= cursor_byte
                        && a.name_start + a.name_len >= cursor_byte
                });

                // Show modifiers if cursor is after __ in an existing key.
                // Also show modifiers when raw_name ends with _ (user just typed first
                // underscore of __, InsertCharPre fires before the second _ is inserted).
                let show_modifiers = key.as_ref().is_some_and(|k| {
                    !k.is_empty()
                        && matching_attr.is_some_and(|a| {
                            let has_double = a.raw_name.contains("__")
                                && cursor_byte > a.name_start + a.raw_name.find("__").unwrap_or(0);
                            let has_single_trailing =
                                a.raw_name.ends_with('_') && !a.raw_name.contains("__");
                            has_double || has_single_trailing
                        })
                });

                if show_modifiers {
                    let used_mods = matching_attr.map(|a| &a.modifiers);
                    items.extend(complete_modifiers(def, used_mods.unwrap_or(&Vec::new())));
                } else {
                    items.extend(complete_keys(&plugin_name));
                }
            }
        }
        CursorPosition::AttributeValue {
            value_start,
            full_value,
            ..
        }
        | CursorPosition::AttrsPropValue {
            value_start,
            full_value,
            ..
        } => {
            // Cursor is inside a data-* attribute value
            value_completions(&mut items, &attrs, cursor_byte, value_start, &full_value);
        }
        CursorPosition::InMarkup => {
            // Cursor just past a data-* attribute name — check for key/modifier completions.
            // e.g. <button data-on:click__>| or <input data-bind: |>|
            let just_after = attrs.iter().find(|a| {
                let name_end = a.name_start + a.name_len;
                cursor_byte >= name_end && cursor_byte <= name_end + 3
            });
            if let Some(attr) = just_after {
                if let Some(def) = attributes::all().get(attr.plugin_name.as_str()) {
                    let has_colon = attr.raw_name.contains(':') || attr.has_trailing_colon;
                    let has_modifiers = attr.raw_name.contains("__");
                    let has_key = attr.key.as_deref().is_some_and(|k| !k.is_empty());

                    if has_modifiers && has_key {
                        items.extend(complete_modifiers(def, &attr.modifiers));
                        return deduplicate_and_sort(items);
                    }
                    if has_colon && !has_key {
                        items.extend(complete_keys(&attr.plugin_name));
                        return deduplicate_and_sort(items);
                    }
                }
            }
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
    use crate::analysis::value_scanner::{scan_value, signal_at_cursor, SpanKind};

    let rel = cursor_byte.saturating_sub(value_start);
    if rel > full_value.len() {
        return;
    }

    // Cursor at/past end of value — check what the value starts with
    if rel >= full_value.len() {
        if full_value.starts_with("evt.") || full_value == "evt" {
            items.extend(complete_evt_props(attrs));
            return;
        }
        if full_value.starts_with('@') {
            items.extend(complete_actions());
        } else if full_value.starts_with('$') {
            items.extend(complete_signals(attrs));
        }
        return;
    }

    let before = &full_value[..rel];

    // evt. property completions — type-narrowed by event key
    if before.ends_with("evt.") || before.ends_with("evt") {
        items.extend(complete_evt_props(attrs));
        return;
    }
    let evt_pos = before.rfind("evt.");

    // Check if cursor is on a $ or @ span
    if let Some(span) = scan_value(full_value)
        .iter()
        .find(|s| rel >= s.start && rel <= s.end)
    {
        match span.kind {
            SpanKind::DollarSignal => {
                // Show signals when cursor is on a $signal
                items.extend(complete_signals(attrs));
                return;
            }
            SpanKind::AtAction => {
                items.extend(complete_actions());
                return;
            }
            SpanKind::EvtDotProp => {
                if let Some(epos) = evt_pos {
                    let prefix = &before[epos + 4..];
                    if !prefix.contains(' ') && !prefix.contains(')') {
                        items.extend(complete_evt_props_filtered(attrs, Some(prefix)));
                        return;
                    }
                }
                items.extend(complete_evt_props(attrs));
                return;
            }
        }
    }

    // Cursor just after $ or @ with no identifier yet
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

    // Try signal_at_cursor for backtracking
    if let Some(name) = signal_at_cursor(full_value, rel) {
        let top = name.split('.').next().unwrap_or("");
        // Only add if not already handled
        if !items.iter().any(|i| i.label == format!("${top}")) {
            items.extend(complete_signals_filtered(attrs, Some(top)));
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

/// Complete keys for a plugin (e.g. event names for `data-on:`, CSS class names for `data-class:`).
fn complete_keys(plugin_name: &str) -> Vec<CompletionItem> {
    match plugin_name {
        "on" => KNOWN_DOM_EVENTS
            .iter()
            .map(|event| CompletionItem {
                label: event.to_string(),
                kind: Some(CompletionItemKind::EVENT),
                detail: Some("DOM event".to_string()),
                insert_text: Some(format!("{event}=\"${{1:expression}}\"")),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            })
            .collect(),
        "bind" => vec![
            key_item("value", "Input value binding"),
            key_item("checked", "Checkbox/radio checked state"),
        ],
        "attr" => vec![
            key_item("disabled", "Disable the element"),
            key_item("href", "Link URL"),
            key_item("src", "Image/source URL"),
            key_item("class", "CSS class name"),
            key_item("id", "Element ID"),
            key_item("style", "Inline style"),
            key_item("title", "Tooltip text"),
            key_item("type", "Input type"),
            key_item("placeholder", "Placeholder text"),
            key_item("aria-label", "Accessibility label"),
        ],
        "class" => vec![
            key_item("active", "Toggle active state"),
            key_item("hidden", "Toggle visibility"),
            key_item("disabled", "Toggle disabled appearance"),
            key_item("loading", "Toggle loading state"),
            key_item("error", "Toggle error state"),
            key_item("selected", "Toggle selection"),
        ],
        _ => vec![],
    }
}

fn key_item(name: &str, desc: &str) -> CompletionItem {
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::ENUM_MEMBER),
        detail: Some(desc.to_string()),
        ..Default::default()
    }
}

fn complete_evt_props(attrs: &[AttrData]) -> Vec<CompletionItem> {
    complete_evt_props_filtered(attrs, None)
}

fn complete_evt_props_filtered(attrs: &[AttrData], prefix: Option<&str>) -> Vec<CompletionItem> {
    let event_key = attrs
        .iter()
        .find(|a| a.plugin_name == "on")
        .and_then(|a| a.key.as_deref());

    crate::analysis::events::properties_for(event_key.unwrap_or("click"))
        .into_iter()
        .filter(|p| {
            prefix.is_none_or(|pf| {
                p.name.to_lowercase().starts_with(&pf.to_lowercase()) && p.name.len() > pf.len()
            })
        })
        .map(|p| CompletionItem {
            label: format!("evt.{}", p.name),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(p.desc.to_string()),
            insert_text: Some(p.name.to_string()),
            ..Default::default()
        })
        .collect()
}

fn complete_signals_filtered(attrs: &[AttrData], prefix: Option<&str>) -> Vec<CompletionItem> {
    let mut seen = std::collections::BTreeSet::new();
    attrs
        .iter()
        .flat_map(crate::analysis::signal_util::signal_names_from_attr)
        .filter(|name| seen.insert(name.clone()))
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
    fn test_complete_signals_from_value() {
        let html = r#"<div data-bind="percentage"><span data-text="$"></span></div>"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_html::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(html, None).unwrap();
        let attrs = crate::analysis::ts_util::collect_from_tree(tree.root_node(), html);
        let items = complete_signals(&attrs);
        assert!(
            items.iter().any(|s| s.label == "$percentage"),
            "got: {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_complete_signals_from_obj_literal() {
        let html = r#"<div data-signals="{foo: 1, bar: 2}"><span data-text="$"></span></div>"#;
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

    #[test]
    fn test_complete_event_keys() {
        let html = r#"<button data-on:></button>"#;
        let uri = Url::parse("file:///test.html").unwrap();
        let li = LineIndex::new(html.to_string());
        // `<button data-on:></button>` — colon at byte 15
        let (line, col) = li.byte_to_position(15);
        let items = generate(
            &li,
            html,
            Position {
                line,
                character: col,
            },
            &uri,
        );
        assert!(
            items.iter().any(|i| i.label == "click"),
            "should suggest event names, got: {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
    }
}
