use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::analysis::project_index::ProjectIndex;
use crate::analysis::signals::{self, SignalAnalysis};
use crate::line_index::LineIndex;

/// Find all references to the signal or action at the given position.
/// Searches local document first, then cross-file index.
pub fn find_references(
    line_index: &LineIndex,
    position: Position,
    uri: &Url,
    analysis: &SignalAnalysis,
    project_index: Option<&ProjectIndex>,
) -> Vec<Location> {
    let offset = line_index.position_to_byte_offset(position.line, position.character);
    let bytes = line_index.text().as_bytes();

    // Find which signal reference the cursor is on
    let cursor_name = signals::find_signal_name_at_offset(bytes, offset);

    let top_name = match cursor_name {
        Some(ref name) => name.split('.').next().unwrap_or("").to_string(),
        None => return vec![],
    };

    if top_name.is_empty() {
        return vec![];
    }

    // Check if signal is known (locally or cross-file)
    let signal_exists = analysis.top_level_names.contains(&top_name)
        || project_index
            .map(|idx| !idx.find_definitions(&top_name, None).is_empty())
            .unwrap_or(false);

    if !signal_exists {
        return vec![];
    }

    let mut locations = Vec::new();

    // Add definition locations
    if let Some(defs) = analysis.definitions.get(&top_name) {
        for def in defs {
            let (line, col) = line_index.byte_to_position(def.byte_offset);
            let pos = tower_lsp::lsp_types::Position {
                line,
                character: col,
            };
            locations.push(Location {
                uri: uri.clone(),
                range: Range {
                    start: pos,
                    end: Position {
                        line: pos.line,
                        character: pos.character + def.name.len() as u32 + 1,
                    },
                },
            });
        }
    }

    // Add reference locations (all $top_name usages)
    for ref_ in &analysis.references {
        let ref_top = ref_.name.split('.').next().unwrap_or("");
        if ref_top == top_name {
            let (line, col) = line_index.byte_to_position(ref_.byte_offset);
            let pos = tower_lsp::lsp_types::Position {
                line,
                character: col,
            };
            locations.push(Location {
                uri: uri.clone(),
                range: Range {
                    start: pos,
                    end: Position {
                        line: pos.line,
                        character: pos.character + ref_.name.len() as u32 + 1,
                    },
                },
            });
        }
    }

    // Add cross-file references from project index
    if let Some(index) = project_index {
        for (cross_uri, byte_offset, len) in index.find_all_references(&top_name) {
            if &cross_uri == uri {
                continue; // Already added locally
            }
            if let Some((line_index, _, _)) = index.get(&cross_uri) {
                let (line, col) = line_index.byte_to_position(byte_offset);
                let pos = tower_lsp::lsp_types::Position {
                    line,
                    character: col,
                };
                locations.push(Location {
                    uri: cross_uri.clone(),
                    range: Range {
                        start: pos,
                        end: Position {
                            line: pos.line,
                            character: pos.character + len as u32,
                        },
                    },
                });
            }
        }
    }

    locations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_references() {
        let html = r#"<div data-signals:counter="0"><span data-text="$counter"></span><button data-on:click="$counter++">+</button></div>"#;
        let uri = Url::parse("file:///test.html").unwrap();

        let dollar_pos = html.find("$counter").unwrap();
        let pos = Position {
            line: 0,
            character: dollar_pos as u32 + 1,
        };

        let line_index = crate::line_index::LineIndex::new(html.to_string());
        let analysis = crate::analysis::signals::analyze_signals(html);
        let locs = find_references(&line_index, pos, &uri, &analysis, None);
        // Should find: definition at data-signals:counter, ref at $counter in data-text, ref at $counter++ in data-on
        assert!(
            locs.len() >= 3,
            "expected >=3 references, got {}",
            locs.len()
        );
    }

    #[test]
    fn test_no_references_for_undefined() {
        let html = r#"<div data-text="$foo"></div>"#;
        let uri = Url::parse("file:///test.html").unwrap();

        let dollar_pos = html.find("$foo").unwrap();
        let pos = Position {
            line: 0,
            character: dollar_pos as u32 + 1,
        };

        let line_index = crate::line_index::LineIndex::new(html.to_string());
        let analysis = crate::analysis::signals::analyze_signals(html);
        let locs = find_references(&line_index, pos, &uri, &analysis, None);
        assert!(
            locs.is_empty(),
            "undefined signal should have no references"
        );
    }
}
