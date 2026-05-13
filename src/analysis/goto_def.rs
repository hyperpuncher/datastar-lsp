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
    let (_, attrs) = ts_util::parse_and_collect(text, uri)?;

    let signal_name = signal_util::find_signal_at_cursor(&attrs, offset)?;
    let top = signal_name.split('.').next().unwrap_or("");

    // Local definition
    for def_attr in &attrs {
        if DEFINERS.contains(&def_attr.plugin_name.as_str())
            && signal_util::signal_names_from_attr(def_attr)
                .iter()
                .any(|n| n == top)
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
        if let Some(def) = signal_util::index_find_def_entry(index, top)
            .and_then(|e| signal_util::def_entry_to_location(index, &e))
        {
            return Some(GotoDefinitionResponse::Scalar(def));
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

    #[test]
    fn test_goto_bind_value_definition() {
        let html = r#"<input data-bind="percentage" /><button data-on:click="$percentage = 50">Set</button>"#;
        assert!(gd_for(html, "$percentage").is_some());
    }
}
