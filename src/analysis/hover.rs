use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Url};

use crate::analysis::signal_util;
use crate::analysis::ts_util;
use crate::data::{actions, attributes};
use crate::line_index::LineIndex;

pub fn generate(
    line_index: &LineIndex,
    text: &str,
    position: Position,
    uri: &Url,
) -> Option<Hover> {
    let offset = line_index.position_to_byte_offset(position.line, position.character);

    let (_, attrs) = ts_util::parse_and_collect(text, uri)?;

    for attr in &attrs {
        if offset >= attr.name_start && offset <= attr.name_start + attr.name_len {
            return hover_attribute_node(attr);
        }
        if let (Some(value_start), Some(value)) = (attr.value_start, &attr.value) {
            let value_end = value_start + value.len();
            if offset >= value_start && offset <= value_end {
                let rel = offset.saturating_sub(value_start);
                if rel < value.len() {
                    return hover_value_text(value, rel, &attrs);
                }
            }
        }
    }

    None
}

fn hover_attribute_node(attr: &crate::analysis::ts_util::AttrData) -> Option<Hover> {
    let registry = attributes::all();
    let def = registry.get(attr.plugin_name.as_str())?;

    let mut content = format!("## `data-{}`\n\n{}", attr.plugin_name, def.description);

    if let Some(key) = &attr.key {
        content.push_str(&format!("\n\n**Key:** `{key}`"));
    }

    if !attr.modifiers.is_empty() {
        content.push_str("\n\n**Modifiers:**");
        for (mod_key, tags) in &attr.modifiers {
            if tags.is_empty() {
                content.push_str(&format!("\n- `__{mod_key}`"));
            } else {
                content.push_str(&format!("\n- `__{mod_key}.{}`", tags.join(".")));
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
        "\n\n**Key:** {key_info} | **Value:** {value_info}"
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

fn hover_value_text(
    value: &str,
    rel: usize,
    attrs: &[crate::analysis::ts_util::AttrData],
) -> Option<Hover> {
    let bytes = value.as_bytes();
    if rel >= bytes.len() {
        return None;
    }

    // Direct $ hit: read signal name immediately
    if bytes[rel] == b'$' {
        return signal_util::read_signal_name(&value[rel + 1..])
            .and_then(|n| hover_signal(&n, attrs));
    }

    // Direct @ hit: read action name
    if bytes[rel] == b'@' {
        return hover_action_name(value, rel);
    }

    // Alpha-ish char: backtrack to find leading $ or @
    if bytes[rel].is_ascii_alphanumeric()
        || bytes[rel] == b'_'
        || bytes[rel] == b'-'
        || bytes[rel] == b'.'
    {
        let mut start = rel;
        while start > 0 {
            let c = bytes[start - 1];
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'.' {
                start -= 1;
            } else {
                break;
            }
        }
        if start > 0 && bytes[start - 1] == b'$' {
            return signal_util::read_signal_name(&value[start..])
                .and_then(|n| hover_signal(&n, attrs));
        }
        if start > 0 && bytes[start - 1] == b'@' {
            return hover_action_name(value, start - 1);
        }
    }

    None
}

fn hover_signal(name: &str, attrs: &[crate::analysis::ts_util::AttrData]) -> Option<Hover> {
    let top = name.split('.').next().unwrap_or("");

    if top == "evt" {
        return mk_hover(
            "## `$evt`\n\nBuilt-in signal: the current event object.\n\nAvailable in `data-on:*` expressions.",
        );
    }
    if top == "el" {
        return mk_hover(
            "## `$el`\n\nBuilt-in signal: the element on which the attribute resides.",
        );
    }

    if signal_util::is_defined(top, attrs, None) {
        mk_hover(&format!("## `${name}`\n\nSignal defined in this document."))
    } else {
        mk_hover(&format!(
            "## `${name}`\n\n⚠️ **Undefined signal**: `${{{name}}}` is not defined in this document.",
            name = name
        ))
    }
}

fn hover_action_name(value: &str, offset: usize) -> Option<Hover> {
    let bytes = value.as_bytes();
    let mut end = offset + 1;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    let name = std::str::from_utf8(&bytes[offset + 1..end]).unwrap_or("");
    let registry = actions::all();

    if let Some(def) = registry.get(name) {
        let params = def.params.join(", ");
        let content = format!(
            "## `@{name}`\n\n{}\n\n**Signature:** `@{name}`({params})\n\n[Documentation]({})",
            def.description, def.doc_url
        );
        mk_hover(&content)
    } else {
        mk_hover(&format!(
            "## `@{name}`\n\n⚠️ **Unknown action**: `@{name}` is not a recognized Datastar action."
        ))
    }
}

fn mk_hover(markdown: &str) -> Option<Hover> {
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown.to_string(),
        }),
        range: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hover_at(html: &str, cursor_pattern: &str) -> Option<Hover> {
        let uri = Url::parse("file:///test.html").unwrap();
        let offset = html.find(cursor_pattern).unwrap();
        let li = LineIndex::new(html.to_string());
        let (line, col) = li.byte_to_position(offset);
        generate(
            &li,
            html,
            Position {
                line,
                character: col,
            },
            &uri,
        )
    }

    #[test]
    fn test_hover_on_signal() {
        let html = r#"<div data-signals:foo="1"><span data-text="$foo"></span></div>"#;
        let h = hover_at(html, "$foo").expect("hover for $foo");
        let v = match &h.contents {
            HoverContents::Markup(m) => &m.value,
            _ => panic!("expected markup"),
        };
        assert!(v.contains("foo"), "hover: {v}");
    }

    #[test]
    fn test_hover_on_attribute() {
        let uri = Url::parse("file:///test.html").unwrap();
        let html = r#"<div data-show="true"></div>"#;
        let li = LineIndex::new(html.to_string());
        let h = generate(
            &li,
            html,
            Position {
                line: 0,
                character: 7,
            },
            &uri,
        )
        .expect("hover at char 7");
        let v = match &h.contents {
            HoverContents::Markup(m) => &m.value,
            _ => panic!("expected markup"),
        };
        assert!(v.contains("data-show"), "hover: {v}");
    }

    #[test]
    fn test_hover_on_count_decrement() {
        let uri = Url::parse("file:///test.html").unwrap();
        let html = r#"<input data-bind:count /><button data-on:click="$count--">-</button>"#;
        let offset = html.find("$count").unwrap();
        let li = LineIndex::new(html.to_string());
        let (line, col) = li.byte_to_position(offset);
        let h = generate(
            &li,
            html,
            Position {
                line,
                character: col,
            },
            &uri,
        )
        .expect("hover for $count");
        let v = match &h.contents {
            HoverContents::Markup(m) => &m.value,
            _ => panic!("expected markup"),
        };
        assert!(!v.contains("Undefined"), "should be defined, got: {v}");
    }

    #[test]
    fn test_hover_on_minus_sign() {
        let uri = Url::parse("file:///test.html").unwrap();
        let html = r#"<input data-bind:count /><button data-on:click="$count--">-</button>"#;
        let offset = html.find("$count--").unwrap() + 6;
        let li = LineIndex::new(html.to_string());
        let (line, col) = li.byte_to_position(offset);
        let h = generate(
            &li,
            html,
            Position {
                line,
                character: col,
            },
            &uri,
        );
        if let Some(h) = h {
            let v = match &h.contents {
                HoverContents::Markup(m) => &m.value,
                _ => panic!("expected markup"),
            };
            assert!(!v.contains("Undefined"), "should be defined, got: {v}");
        }
    }
}
