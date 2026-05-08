use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Position, Range, Url};

use crate::analysis::project_index::ProjectIndex;
use crate::analysis::signals::{self, SignalAnalysis};
use crate::line_index::LineIndex;
use crate::parser::html::DataAttribute;

/// Find the definition location of a signal or action at the given position.
/// Searches local document first, then cross-file index.
pub fn goto_definition(
    line_index: &LineIndex,
    position: Position,
    uri: &Url,
    attrs: &[DataAttribute],
    analysis: &SignalAnalysis,
    project_index: Option<&ProjectIndex>,
) -> Option<GotoDefinitionResponse> {
    let offset = line_index.position_to_byte_offset(position.line, position.character);

    // Check if cursor is inside a signal reference in an attribute value
    for attr in attrs {
        let rel_offset = match attr.value_rel_offset(offset) {
            Some(r) => r,
            None => continue,
        };

        let bytes = attr.value.as_ref().map(|v| v.as_bytes()).unwrap_or(b"");

        if let Some(signal_name) = signals::find_signal_name_at_offset(bytes, rel_offset) {
            let top_name = signal_name.split('.').next().unwrap_or("");
            return find_signal_definition(line_index, top_name, uri, analysis, project_index);
        }
    }

    None
}

/// Find the definition of a top-level signal name in the document.
/// Falls back to cross-file project index if not found locally.
fn find_signal_definition(
    line_index: &LineIndex,
    top_name: &str,
    uri: &Url,
    analysis: &SignalAnalysis,
    project_index: Option<&ProjectIndex>,
) -> Option<GotoDefinitionResponse> {
    // Try local first
    if let Some(defs) = analysis.definitions.get(top_name) {
        let def = defs.first()?;
        let (line, col) = line_index.byte_to_position(def.byte_offset);
        let pos = tower_lsp::lsp_types::Position {
            line,
            character: col,
        };
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: Range {
                start: pos,
                end: Position {
                    line: pos.line,
                    character: pos.character + def.name.len() as u32 + 1,
                },
            },
        }));
    }

    // Fall back to cross-file index
    if let Some(index) = project_index {
        let cross_defs = index.find_definitions(top_name, Some(uri));
        if !cross_defs.is_empty() {
            let (cross_uri, byte_offset) = &cross_defs[0];
            if let Some((line_index, _, _)) = index.get(cross_uri) {
                let (line, col) = line_index.byte_to_position(*byte_offset);
                let pos = tower_lsp::lsp_types::Position {
                    line,
                    character: col,
                };
                return Some(GotoDefinitionResponse::Scalar(Location {
                    uri: cross_uri.clone(),
                    range: Range {
                        start: pos,
                        end: Position {
                            line: pos.line,
                            character: pos.character + top_name.len() as u32 + 1,
                        },
                    },
                }));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gd_for(html: &str, cursor: &str) -> Option<GotoDefinitionResponse> {
        let parsed = crate::parser::html::parse_html(html.as_bytes()).unwrap();
        let uri = Url::parse("file:///test.html").unwrap();
        let offset = html.find(cursor).unwrap();
        let line_index = crate::line_index::LineIndex::new(html.to_string());
        let (line, col) = line_index.byte_to_position(offset);
        let pos = Position {
            line,
            character: col + 1,
        }; // inside signal name
        let analysis = crate::analysis::signals::analyze_signals(html);
        goto_definition(&line_index, pos, &uri, &parsed.1, &analysis, None)
    }

    #[test]
    fn test_goto_signal_definition() {
        let html = r#"<div data-signals:foo="1"><span data-text="$foo"></span></div>"#;
        let result = gd_for(html, "$foo");
        assert!(result.is_some(), "should find definition for $foo");
    }

    #[test]
    fn test_goto_undefined_signal() {
        let html = r#"<div data-text="$nonexistent"></div>"#;
        let result = gd_for(html, "$nonexistent");
        assert!(
            result.is_none(),
            "undefined signal should have no definition"
        );
    }

    #[test]
    fn test_goto_bind_definition() {
        let html = r#"<input data-bind:count /><button data-on:click="$count++">+</button>"#;
        let result = gd_for(html, "$count");
        assert!(result.is_some(), "should find definition for $count");
    }
}
