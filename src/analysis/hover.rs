use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::analysis::signals;
use crate::data::{actions, attributes};
use crate::line_index::LineIndex;
use crate::parser::html::DataAttribute;

/// Generate hover information for a position in the document.
pub fn generate(
    line_index: &LineIndex,
    position: Position,
    attrs: &[DataAttribute],
) -> Option<Hover> {
    let offset = line_index.position_to_byte_offset(position.line, position.character);

    for attr in attrs {
        if offset >= attr.name_start && offset <= attr.name_start + attr.raw_name.len() {
            return hover_attribute(attr);
        }
        if let (Some(value_start), Some(value)) = (attr.value_start, &attr.value) {
            let value_end = value_start + value.len() + 2;
            if offset >= value_start && offset <= value_end {
                return hover_value(
                    attr,
                    offset.saturating_sub(value_start + 1),
                    line_index.text(),
                );
            }
        }
    }

    None
}

fn hover_attribute(attr: &DataAttribute) -> Option<Hover> {
    let registry = attributes::all();
    let def = registry.get(attr.plugin_name.as_str())?;

    let mut content = format!("## `data-{}`\n\n{}", attr.plugin_name, def.description);

    if let Some(key) = &attr.key {
        content.push_str(&format!("\n\n**Key:** `{}`", key));
    }

    if !attr.modifiers.is_empty() {
        content.push_str("\n\n**Modifiers:**");
        for (mod_key, tags) in &attr.modifiers {
            if tags.is_empty() {
                content.push_str(&format!("\n- `__{}`", mod_key));
            } else {
                content.push_str(&format!("\n- `__{}.{}`", mod_key, tags.join(".")));
            }
        }
    }

    let key_info = match def.key_req {
        attributes::Requirement::Must => "required",
        attributes::Requirement::Allowed => "optional",
        attributes::Requirement::Exclusive => "exclusive with value",
        attributes::Requirement::Denied => "not allowed",
    };
    let value_info = match def.value_req {
        attributes::Requirement::Must => "required",
        attributes::Requirement::Allowed => "optional",
        attributes::Requirement::Exclusive => "exclusive with key",
        attributes::Requirement::Denied => "not allowed",
    };
    content.push_str(&format!(
        "\n\n**Key:** {} | **Value:** {}",
        key_info, value_info
    ));

    if def.pro {
        content.push_str("\n\n> ⚠️ Datastar Pro attribute");
    }

    content.push_str(&format!("\n\n[Documentation]({})", def.doc_url));

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: None,
    })
}

fn hover_value(attr: &DataAttribute, value_offset: usize, full_text: &str) -> Option<Hover> {
    let value = attr.value.as_ref()?;
    let bytes = value.as_bytes();

    if value_offset < bytes.len() && bytes[value_offset] == b'$' {
        let (name, _) = signals::read_signal_token(bytes, value_offset + 1);
        if !name.is_empty() {
            return hover_signal_name(name, full_text);
        }
        return None;
    }

    if value_offset < bytes.len() && bytes[value_offset] == b'@' {
        return hover_action(value, value_offset);
    }

    if let Some(sig_content) = signals::find_signal_name_at_offset(bytes, value_offset) {
        return hover_signal_name(&sig_content, full_text);
    }

    None
}

fn hover_signal_name(name: &str, full_text: &str) -> Option<Hover> {
    let analysis = crate::analysis::signals::analyze_signals(full_text);
    let top_name = name.split('.').next().unwrap_or("");

    if let Some(defs) = analysis.definitions.get(top_name) {
        let def_by = defs
            .first()
            .map(|d| d.defined_by.as_str())
            .unwrap_or("unknown");

        let content = format!("## `${{{name}}}`\n\nSignal defined via `data-{def_by}`.");

        if defs.len() > 1 {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "{}\n\n> Defined {} times in this document.",
                        content,
                        defs.len()
                    ),
                }),
                range: None,
            });
        }

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: None,
        });
    }

    if top_name == "evt" {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "## `$evt`\n\nBuilt-in signal: the current event object.\n\nAvailable in `data-on:*` expressions."
                    .to_string(),
            }),
            range: None,
        });
    }
    if top_name == "el" {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "## `$el`\n\nBuilt-in signal: the element on which the attribute resides."
                    .to_string(),
            }),
            range: None,
        });
    }

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!(
                "## `${{{n}}}`\n\n⚠️ **Undefined signal**: `${{{n}}}` is not defined in this document.",
                n = name
            ),
        }),
        range: None,
    })
}

fn hover_action(value: &str, offset: usize) -> Option<Hover> {
    let bytes = value.as_bytes();
    let mut end = offset + 1;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    let action_name = std::str::from_utf8(&bytes[offset + 1..end]).unwrap_or("");
    let registry = actions::all();

    if let Some(def) = registry.get(action_name) {
        let params = def.params.join(", ");
        let mut content = format!(
            "## `@{}`\n\n{}\n\n**Signature:** `@{}`({})",
            action_name, def.description, action_name, params
        );

        if def.pro {
            content.push_str("\n\n> ⚠️ Datastar Pro action");
        }

        content.push_str(&format!("\n\n[Documentation]({})", def.doc_url));

        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: None,
        })
    } else {
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "## `@{}`\n\n⚠️ **Unknown action**: `@{}` is not a recognized Datastar action.",
                    action_name, action_name
                ),
            }),
            range: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hover_on_signal() {
        let html = r#"<div data-signals:foo="1"><span data-text="$foo"></span></div>"#;
        let parsed = crate::parser::html::parse_html(html.as_bytes()).unwrap();
        // Find byte offset of $foo
        let dollar_offset = html.find("$foo").unwrap();
        let pos = Position {
            line: 0,
            character: dollar_offset as u32,
        };
        let hover = generate(&LineIndex::new(html.to_string()), pos, &parsed.1);
        assert!(
            hover.is_some(),
            "expected hover at offset {}",
            dollar_offset
        );
        let hover = hover.unwrap();
        let contents = match &hover.contents {
            HoverContents::Markup(m) => &m.value,
            _ => panic!("expected markup"),
        };
        assert!(
            contents.contains("foo"),
            "hover should mention foo: {}",
            contents
        );
    }

    #[test]
    fn test_hover_on_attribute() {
        let html = r#"<div data-show="true"></div>"#;
        let parsed = crate::parser::html::parse_html(html.as_bytes()).unwrap();
        let pos = Position {
            line: 0,
            character: 7,
        };
        let hover = generate(&LineIndex::new(html.to_string()), pos, &parsed.1);
        assert!(hover.is_some(), "expected hover at char 7");
        let hover = hover.unwrap();
        let contents = match &hover.contents {
            HoverContents::Markup(m) => &m.value,
            _ => panic!("expected markup"),
        };
        assert!(
            contents.contains("data-show"),
            "hover should mention data-show: {}",
            contents
        );
    }
}
