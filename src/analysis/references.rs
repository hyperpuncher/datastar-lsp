use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::analysis::ts_util::AttrData;
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

    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .is_err()
    {
        return vec![];
    }
    let tree = match parser.parse(text, None) {
        Some(t) => t,
        None => return vec![],
    };
    let attrs = crate::analysis::ts_util::collect_from_tree(tree.root_node(), text);

    let signal_name = if let Some(name) = find_signal_at_cursor(&attrs, offset) {
        name
    } else {
        return vec![];
    };
    let top = signal_name.split('.').next().unwrap_or("");

    let definers: std::collections::BTreeSet<&str> = [
        "signals",
        "bind",
        "computed",
        "ref",
        "indicator",
        "match-media",
    ]
    .iter()
    .copied()
    .collect();

    let is_defined = attrs
        .iter()
        .filter(|a| definers.contains(a.plugin_name.as_str()))
        .any(|a| a.key.as_deref() == Some(top))
        || project_index.as_ref().is_some_and(|idx| {
            idx.iter().any(|e| {
                let (_li, t) = e.value();
                t.contains(&format!("data-signals:{top}"))
                    || t.contains(&format!("data-bind:{top}"))
            })
        });
    if !is_defined {
        return vec![];
    }

    let mut locations = Vec::new();

    // Local definitions
    for attr in &attrs {
        if definers.contains(attr.plugin_name.as_str()) && attr.key.as_deref() == Some(top) {
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
            let byte_pos = value_start + 1 + pos + (value.len() - search.len());
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

    // Cross-file
    if let Some(index) = project_index {
        for entry in index.iter() {
            let (cross_li, cross_text) = entry.value();
            if entry.key() == uri {
                continue;
            }
            for prefix in &[
                "data-signals:",
                "data-bind:",
                "data-computed:",
                "data-ref:",
                "data-indicator:",
            ] {
                let pattern = format!("{prefix}{top}");
                for (pos, _) in cross_text.match_indices(&pattern) {
                    let (line, col) = cross_li.byte_to_position(pos + prefix.len());
                    locations.push(Location {
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
                    });
                }
            }
            let dollar = format!("${top}");
            for (pos, _) in cross_text.match_indices(&dollar) {
                let (line, col) = cross_li.byte_to_position(pos);
                locations.push(Location {
                    uri: entry.key().clone(),
                    range: Range {
                        start: Position {
                            line,
                            character: col,
                        },
                        end: Position {
                            line,
                            character: col + dollar.len() as u32,
                        },
                    },
                });
            }
        }
    }

    locations
}

fn find_signal_at_cursor(attrs: &[AttrData], offset: usize) -> Option<String> {
    for attr in attrs {
        let (Some(value_start), Some(value)) = (attr.value_start, &attr.value) else {
            continue;
        };
        let value_end = value_start + value.len() + 2;
        if offset < value_start || offset > value_end {
            continue;
        }
        let rel = offset.saturating_sub(value_start + 1);
        if rel >= value.len() {
            return None;
        }
        let bytes = value.as_bytes();
        if bytes[rel] == b'$' {
            return read_signal_name(&value[rel + 1..]);
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
                return read_signal_name(&value[start..]);
            }
        }
    }
    None
}

fn read_signal_name(s: &str) -> Option<String> {
    let end = s
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-' && c != '.')
        .unwrap_or(s.len());
    let raw = &s[..end];
    let trimmed = raw
        .trim_end_matches("++")
        .trim_end_matches("--")
        .trim_end_matches('.');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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
}
