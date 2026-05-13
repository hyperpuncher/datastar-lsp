use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::analysis::signal_util::{self, DEFINERS, DEFINER_PREFIXES};
use crate::analysis::ts_util;
use crate::line_index::LineIndex;

/// Find all references to a signal at the given position.
pub fn find_references(
    line_index: &LineIndex,
    text: &str,
    position: Position,
    uri: &Url,
    project_index: Option<&crate::analysis::project_index::ProjectIndex>,
) -> Vec<Location> {
    let offset = line_index.position_to_byte_offset(position.line, position.character);

    let Some((_, attrs)) = ts_util::parse_and_collect(text, uri) else {
        return vec![];
    };

    let signal_name = match signal_util::find_signal_at_cursor(&attrs, offset) {
        Some(name) => name,
        None => return vec![],
    };
    let top = signal_name.split('.').next().unwrap_or("");

    if !signal_util::is_defined(top, &attrs, project_index) {
        return vec![];
    }

    let mut locations = Vec::new();

    // Local definitions
    for attr in &attrs {
        if DEFINERS.contains(&attr.plugin_name.as_str())
            && signal_util::signal_names_from_attr(attr)
                .iter()
                .any(|n| n == top)
        {
            let (line, col) = line_index.byte_to_position(attr.name_start);
            locations.push(Location {
                uri: uri.clone(),
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
            });
        }
    }

    // Local $references
    for attr in &attrs {
        let (Some(value_start), Some(value)) = (attr.value_start, &attr.value) else {
            continue;
        };
        let mut search = value.as_str();
        while let Some(pos) = search.find(&format!("${top}")) {
            let byte_pos = value_start + pos + (value.len() - search.len());
            let (line, col) = line_index.byte_to_position(byte_pos);
            locations.push(Location {
                uri: uri.clone(),
                range: Range {
                    start: Position {
                        line,
                        character: col,
                    },
                    end: Position {
                        line,
                        character: col + 1 + top.len() as u32,
                    },
                },
            });
            search = &search[pos + 1..];
        }
    }

    // Cross-file: search for data-{plugin}:{name} and data-{plugin}="{name}"
    if let Some(index) = project_index {
        for entry in index.iter() {
            let cross_li = entry.value();
            let cross_text = cross_li.text();
            if entry.key() == uri {
                continue;
            }
            for prefix in DEFINER_PREFIXES {
                for pattern in [
                    format!("{prefix}:{top}"),
                    format!("{prefix}=\"{top}\""),
                ] {
                    for (pos, _) in cross_text.match_indices(&pattern) {
                        let (line, col) = cross_li.byte_to_position(
                            pos + prefix.len() + 1, // skip "data-<plugin>" and separator
                        );
                        locations.push(Location {
                            uri: entry.key().clone(),
                            range: Range {
                                start: Position { line, character: col },
                                end: Position { line, character: col + top.len() as u32 },
                            },
                        });
                    }
                }
            }
            let dollar = format!("${top}");
            for (pos, _) in cross_text.match_indices(&dollar) {
                let (line, col) = cross_li.byte_to_position(pos);
                locations.push(Location {
                    uri: entry.key().clone(),
                    range: Range {
                        start: Position { line, character: col },
                        end: Position { line, character: col + dollar.len() as u32 },
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

    fn refs_for(html: &str, cursor: &str) -> Vec<Location> {
        let uri = Url::parse("file:///test.html").unwrap();
        let offset = html.find(cursor).unwrap();
        let li = LineIndex::new(html.to_string());
        let (line, col) = li.byte_to_position(offset);
        find_references(
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
    fn test_find_references() {
        let html = r#"<div data-signals:counter="0"><span data-text="$counter"></span><button data-on:click="$counter++">+</button></div>"#;
        let locs = refs_for(html, "$counter");
        assert!(locs.len() >= 3, "got {}", locs.len());
    }

    #[test]
    fn test_no_references_for_undefined() {
        let locs = refs_for(r#"<div data-text="$foo"></div>"#, "$foo");
        assert!(locs.is_empty());
    }

    #[test]
    fn test_find_references_value_based() {
        let html = r#"<input data-bind="percentage" /><button data-on:click="$percentage = 50">Set</button>"#;
        let locs = refs_for(html, "$percentage");
        assert!(locs.len() >= 2, "got {}", locs.len());
    }
}
