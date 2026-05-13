use std::collections::HashMap;

use tower_lsp::lsp_types::{Position, TextEdit, Url};

use crate::analysis::cursor::{self, CursorPosition};
use crate::analysis::signal_util::{self, DEFINERS};
use crate::analysis::ts_util;
use crate::line_index::LineIndex;

/// Rename a signal across the current document and all open files.
pub fn rename_signal(
    line_index: &LineIndex,
    text: &str,
    position: Position,
    uri: &Url,
    new_name: &str,
    project_index: Option<&crate::analysis::project_index::ProjectIndex>,
) -> Option<HashMap<Url, Vec<TextEdit>>> {
    if !signal_util::is_valid_signal_name(new_name) {
        return None;
    }

    let offset = line_index.position_to_byte_offset(position.line, position.character);

    let (tree, attrs) = ts_util::parse_and_collect(text, uri)?;

    let old_name = signal_util::find_signal_at_cursor(&attrs, offset)
        .or_else(|| def_name_from_cursor(tree.root_node(), text, offset))?;
    let top = old_name.split('.').next().unwrap_or("");

    if !signal_util::is_defined(top, &attrs, project_index) {
        return None;
    }

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    let edits = changes.entry(uri.clone()).or_default();

    // Rename definitions in this file
    for attr in &attrs {
        if !DEFINERS.contains(&attr.plugin_name.as_str()) {
            continue;
        }
        if !signal_util::signal_names_from_attr(attr)
            .iter()
            .any(|n| n.as_str() == top)
        {
            continue;
        }
        // Key-based: data-bind:foo → rename the key after colon
        if let Some(key_pos) = attr.raw_name.find(':') {
            let start = attr.name_start + key_pos + 1;
            let (line, col) = line_index.byte_to_position(start);
            edits.push(TextEdit {
                range: tower_lsp::lsp_types::Range {
                    start: Position { line, character: col },
                    end: Position { line, character: col + top.len() as u32 },
                },
                new_text: new_name.to_string(),
            });
            continue;
        }
        // Value-based simple name: data-bind="foo" → rename inside value
        if let Some(value_start) = attr.value_start {
            if let Some(ref val) = attr.value {
                let name = val.trim();
                if name == top {
                    let pos = val.find(top).unwrap_or(0);
                    let start = value_start + pos;
                    let (line, col) = line_index.byte_to_position(start);
                    edits.push(TextEdit {
                        range: tower_lsp::lsp_types::Range {
                            start: Position { line, character: col },
                            end: Position { line, character: col + top.len() as u32 },
                        },
                        new_text: new_name.to_string(),
                    });
                }
            }
        }
    }

    // Rename $references in this file
    for attr in &attrs {
        let (Some(value_start), Some(value)) = (attr.value_start, &attr.value) else {
            continue;
        };
        let mut search = value.as_str();
        while let Some(pos) = search.find(&format!("${top}")) {
            // value_start points into original text; for HTML it's after the opening quote.
            // pos is a byte offset within `value` (the unquoted contents).
            let byte_pos = value_start + pos;
            let (line, col) = line_index.byte_to_position(byte_pos + 1); // +1 to skip $
            edits.push(TextEdit {
                range: tower_lsp::lsp_types::Range {
                    start: Position {
                        line,
                        character: col,
                    },
                    end: Position {
                        line,
                        character: col + top.len() as u32,
                    },
                },
                new_text: new_name.to_string(),
            });
            search = &search[pos + 1..];
        }
    }

    // Cross-file
    if let Some(index) = project_index {
        for entry in index.iter() {
            let cross_uri = entry.key().clone();
            if &cross_uri == uri {
                continue;
            }
            let cross_li = entry.value();
            let cross_text = cross_li.text();
            let cross_edits = changes.entry(cross_uri.clone()).or_default();
            let top_with_dollar = format!("${top}");
            for (pos, _) in cross_text.match_indices(&top_with_dollar) {
                let (line, col) = cross_li.byte_to_position(pos + 1); // +1 to skip $
                cross_edits.push(TextEdit {
                    range: tower_lsp::lsp_types::Range {
                        start: Position {
                            line,
                            character: col,
                        },
                        end: Position {
                            line,
                            character: col + top.len() as u32,
                        },
                    },
                    new_text: new_name.to_string(),
                });
            }
        }
    }

    if changes.is_empty() || changes.values().all(|v| v.is_empty()) {
        return None;
    }
    Some(changes)
}

fn def_name_from_cursor(root: tree_sitter::Node, text: &str, offset: usize) -> Option<String> {
    match cursor::detect(root, text, offset) {
        CursorPosition::AfterColon { key, .. } => key,
        CursorPosition::AttributeName { .. } => {
            // Cursor is on "data-signals:name" — find the : manually
            let bytes = text.as_bytes();
            let mut i = offset as isize;
            while i >= 0 {
                if bytes.get(i as usize) == Some(&b':') {
                    let after = &bytes[i as usize + 1..];
                    let end = after
                        .iter()
                        .position(|b| !b.is_ascii_alphanumeric() && *b != b'-' && *b != b'_')
                        .unwrap_or(after.len());
                    let name = std::str::from_utf8(&after[..end]).unwrap_or("");
                    if !name.is_empty() {
                        return Some(name.to_string());
                    }
                    break;
                }
                i -= 1;
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ren_for(html: &str, cursor: &str, new_name: &str) -> Option<HashMap<Url, Vec<TextEdit>>> {
        let uri = Url::parse("file:///test.html").unwrap();
        let offset = html.find(cursor).unwrap();
        let li = LineIndex::new(html.to_string());
        let (line, col) = li.byte_to_position(offset);
        rename_signal(
            &li,
            html,
            Position {
                line,
                character: col + 1,
            },
            &uri,
            new_name,
            None,
        )
    }

    #[test]
    fn test_rename_signal_on_reference() {
        let html = r#"<div data-signals:counter="0"><span data-text="$counter"></span></div>"#;
        let result = ren_for(html, "$counter", "count");
        assert!(result.is_some());
        let total: usize = result.unwrap().values().map(|v| v.len()).sum();
        assert!(total >= 2);
    }

    #[test]
    fn test_rename_signal_on_definition() {
        let html = r#"<div data-signals:counter="0"><span data-text="$counter"></span></div>"#;
        let result = ren_for(html, ":counter", "count");
        assert!(result.is_some());
    }

    #[test]
    fn test_rename_undefined_signal() {
        let result = ren_for(r#"<div data-text="$foo"></div>"#, "$foo", "bar");
        assert!(result.is_none());
    }

    #[test]
    fn test_rename_invalid_name() {
        let html = r#"<div data-signals:counter="0"><span data-text="$counter"></span></div>"#;
        let result = ren_for(html, "$counter", "my name");
        assert!(result.is_none());
    }

    #[test]
    fn test_rename_value_based() {
        // data-bind="percentage" — value-based definition
        let html = r#"<input data-bind="percentage" /><div data-text="$percentage"></div>"#;
        let result = ren_for(html, "$percentage", "pct");
        assert!(result.is_some());
        let total: usize = result.unwrap().values().map(|v| v.len()).sum();
        assert!(total >= 2, "got {} edits", total);
    }
}
