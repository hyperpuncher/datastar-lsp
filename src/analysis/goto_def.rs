use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Position, Range, Url};

use crate::analysis::signal_util::{self, DEFINERS};
use crate::analysis::ts_util;
use crate::line_index::LineIndex;

/// Find the definition of a signal or action at the given position.
pub fn goto_definition(
    line_index: &LineIndex,
    text: &str,
    position: Position,
    uri: &Url,
    project_index: Option<&crate::analysis::project_index::ProjectIndex>,
) -> Option<GotoDefinitionResponse> {
    let offset = line_index.position_to_byte_offset(position.line, position.character);

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&ts_util::language_for(uri)).ok()?;
    let tree = parser.parse(text, None)?;
    let attrs = crate::analysis::ts_util::collect_from_tree(tree.root_node(), text);

    for attr in &attrs {
        let (Some(value_start), Some(value)) = (attr.value_start, &attr.value) else {
            continue;
        };
        let value_end = value_start + value.len() + 2;
        if offset < value_start || offset > value_end {
            continue;
        }
        let rel = offset.saturating_sub(value_start + 1);
        if rel >= value.len() {
            continue;
        }

        let signal_name = signal_name_at_offset(value, rel)?;
        let top = signal_name.split('.').next().unwrap_or("");

        for def_attr in &attrs {
            if DEFINERS.contains(&def_attr.plugin_name.as_str())
                && def_attr.key.as_deref() == Some(top)
            {
                let pos = line_index.byte_to_position(def_attr.name_start);
                return Some(GotoDefinitionResponse::Scalar(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: Position {
                            line: pos.0,
                            character: pos.1,
                        },
                        end: Position {
                            line: pos.0,
                            character: pos.1 + top.len() as u32,
                        },
                    },
                }));
            }
        }

        // Cross-file
        if let Some(index) = project_index {
            for entry in index.iter() {
                let (cross_li, cross_text) = entry.value();
                for prefix in &[
                    "data-signals:",
                    "data-bind:",
                    "data-computed:",
                    "data-ref:",
                    "data-indicator:",
                ] {
                    let pattern = format!("{prefix}{top}");
                    if let Some(pos) = cross_text.find(&pattern) {
                        let (line, col) = cross_li.byte_to_position(pos + prefix.len());
                        return Some(GotoDefinitionResponse::Scalar(Location {
                            uri: entry.key().clone(),
                            range: Range {
                                start: Position {
                                    line,
                                    character: col,
                                },
                                end: Position {
                                    line,
                                    character: col + top.len() as u32,
                                },
                            },
                        }));
                    }
                }
            }
        }
    }

    None
}

fn signal_name_at_offset(value: &str, rel: usize) -> Option<String> {
    let bytes = value.as_bytes();
    if rel >= bytes.len() {
        return None;
    }
    if bytes[rel] == b'$' {
        return signal_util::read_signal_name(&value[rel + 1..]);
    }
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
            return signal_util::read_signal_name(&value[start..]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gd_for(html: &str, cursor: &str) -> Option<GotoDefinitionResponse> {
        let uri = Url::parse("file:///test.html").unwrap();
        let offset = html.find(cursor).unwrap();
        let li = LineIndex::new(html.to_string());
        let (line, col) = li.byte_to_position(offset);
        goto_definition(
            &li,
            html,
            Position {
                line,
                character: col + 1,
            },
            &uri,
            None,
        )
    }

    #[test]
    fn test_goto_signal_definition() {
        let html = r#"<div data-signals:foo="1"><span data-text="$foo"></span></div>"#;
        assert!(gd_for(html, "$foo").is_some());
    }

    #[test]
    fn test_goto_undefined_signal() {
        let html = r#"<div data-text="$nonexistent"></div>"#;
        assert!(gd_for(html, "$nonexistent").is_none());
    }

    #[test]
    fn test_goto_bind_definition() {
        let html = r#"<input data-bind:count /><button data-on:click="$count++">+</button>"#;
        assert!(gd_for(html, "$count").is_some());
    }
}
