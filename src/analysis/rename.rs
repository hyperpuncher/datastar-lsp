use std::collections::HashMap;

use tower_lsp::lsp_types::{Position, TextEdit, Url};

use crate::analysis::project_index::ProjectIndex;
use crate::analysis::signals::{self, SignalAnalysis};
use crate::line_index::LineIndex;

/// Produce workspace edits to rename a signal across all open documents.
/// Returns edits grouped by URI. Empty if cursor not on a defined signal.
pub fn rename_signal(
    line_index: &LineIndex,
    position: Position,
    uri: &Url,
    new_name: &str,
    analysis: &SignalAnalysis,
    project_index: Option<&ProjectIndex>,
) -> Option<HashMap<Url, Vec<TextEdit>>> {
    let offset = line_index.position_to_byte_offset(position.line, position.character);
    let bytes = line_index.text().as_bytes();

    // Validate new_name is a valid signal identifier
    if !signals::is_valid_signal_name(new_name) {
        return None;
    }

    // Find which signal the cursor is on
    let cursor_name = signals::find_signal_name_at_offset(bytes, offset)
        .or_else(|| find_definition_name_at(bytes, offset));
    let old_name = match cursor_name {
        Some(ref name) => name.split('.').next().unwrap_or("").to_string(),
        None => return None,
    };

    if old_name.is_empty() {
        return None;
    }

    // Check cross-file too
    let signal_exists = analysis.top_level_names.contains(&old_name)
        || project_index
            .map(|idx| !idx.find_definitions(&old_name, None).is_empty())
            .unwrap_or(false);
    if !signal_exists {
        return None;
    }

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    let edits = changes.entry(uri.clone()).or_default();

    // Rename in definitions
    if let Some(defs) = analysis.definitions.get(&old_name) {
        for def in defs {
            if let Some(edit) = make_definition_edit(line_index, def, &old_name, new_name) {
                edits.push(edit);
            }
        }
    }

    // Rename in references: $old_name, $old_name++, $old_name.bar
    for ref_ in &analysis.references {
        let ref_top = ref_.name.split('.').next().unwrap_or("");
        if ref_top == old_name {
            let (line, col) = line_index.byte_to_position(ref_.byte_offset);
            let start = tower_lsp::lsp_types::Position {
                line,
                character: col,
            };
            let end = Position {
                line: start.line,
                character: start.character + ref_.name.len() as u32 + 1,
            };
            edits.push(TextEdit {
                range: tower_lsp::lsp_types::Range { start, end },
                new_text: format!("${}", new_name),
            });
        }
    }

    // Cross-file rename: apply to references only
    if let Some(index) = project_index {
        for entry in index.iter() {
            let cross_uri = entry.key().clone();
            if &cross_uri == uri {
                continue;
            }
            let (line_index, _, analysis) = entry.value();
            let cross_edits = changes.entry(cross_uri.clone()).or_default();
            for ref_ in &analysis.references {
                let ref_top = ref_.name.split('.').next().unwrap_or("");
                if ref_top == old_name {
                    let (line, col) = line_index.byte_to_position(ref_.byte_offset);
                    let pos = tower_lsp::lsp_types::Position {
                        line,
                        character: col,
                    };
                    cross_edits.push(TextEdit {
                        range: tower_lsp::lsp_types::Range {
                            start: pos,
                            end: Position {
                                line: pos.line,
                                character: pos.character + ref_.name.len() as u32 + 1,
                            },
                        },
                        new_text: format!("${}", new_name),
                    });
                }
            }
        }
    }

    if changes.is_empty() || changes.values().all(|v| v.is_empty()) {
        return None;
    }

    Some(changes)
}

/// Create a TextEdit for renaming the signal name in a definition.
fn make_definition_edit(
    line_index: &LineIndex,
    def: &crate::analysis::signals::SignalDef,
    old_name: &str,
    new_name: &str,
) -> Option<TextEdit> {
    let bytes = line_index.text().as_bytes();
    let def_offset = def.byte_offset;

    // Find the colon that separates plugin:name — it's the first colon
    // between "data-" and the first of =, '>' , ' ', or newline
    let after_data = &bytes[def_offset + 5..];
    let eq_or_end = after_data
        .iter()
        .position(|&b| b == b'=' || b == b' ' || b == b'>' || b == b'\n' || b == b'\r')
        .unwrap_or(after_data.len());

    // Find colon within the attribute name bounds
    let colon_pos = after_data[..eq_or_end].iter().position(|&b| b == b':')?;

    // Name starts right after colon, ends at next __ or =/ / >
    let name_start = def_offset + 5 + colon_pos + 1;
    let name_bytes = &bytes[name_start..];
    let name_end = name_bytes
        .iter()
        .position(|&b| b == b'_' || b == b'=' || b == b' ' || b == b'>' || b == b'\n' || b == b'\r')
        .unwrap_or(name_bytes.len());

    if name_end == 0 {
        return None;
    }

    let actual_name = std::str::from_utf8(&name_bytes[..name_end]).unwrap_or("");
    if actual_name != old_name {
        return None;
    }

    let (line, col) = line_index.byte_to_position(name_start);
    let start_pos = tower_lsp::lsp_types::Position {
        line,
        character: col,
    };
    let end_pos = Position {
        line: start_pos.line,
        character: start_pos.character + name_end as u32,
    };

    Some(TextEdit {
        range: tower_lsp::lsp_types::Range {
            start: start_pos,
            end: end_pos,
        },
        new_text: new_name.to_string(),
    })
}

/// Check if cursor is on a signal definition name (after data-signals:FOO).
fn find_definition_name_at(bytes: &[u8], offset: usize) -> Option<String> {
    let mut i = offset;
    while i > 0 {
        if bytes[i] == b':' {
            let before = &bytes[..i];
            let before_str = std::str::from_utf8(before).ok()?;
            let is_definer = before_str.ends_with("data-signals")
                || before_str.ends_with("data-bind")
                || before_str.ends_with("data-computed")
                || before_str.ends_with("data-ref")
                || before_str.ends_with("data-indicator");
            if is_definer {
                let (name, _) = signals::read_signal_token(bytes, i + 1);
                if !name.is_empty() && offset > i && offset <= i + 1 + name.len() {
                    return Some(name.to_string());
                }
            }
            return None;
        }
        i -= 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ren_for(html: &str, cursor: &str, new_name: &str) -> Option<HashMap<Url, Vec<TextEdit>>> {
        let uri = Url::parse("file:///test.html").expect("valid url");
        let offset = html.find(cursor).unwrap();
        let line_index = crate::line_index::LineIndex::new(html.to_string());
        let (line, col) = line_index.byte_to_position(offset);
        let pos = Position {
            line,
            character: col + 1,
        };
        let analysis = crate::analysis::signals::analyze_signals(html);
        rename_signal(&line_index, pos, &uri, new_name, &analysis, None)
    }

    #[test]
    fn test_rename_signal_on_reference() {
        let html = r#"<div data-signals:counter="0"><span data-text="$counter"></span></div>"#;
        let result = ren_for(html, "$counter", "count");
        assert!(result.is_some(), "should produce edits");
        let changes = result.unwrap();
        let total: usize = changes.values().map(|v| v.len()).sum();
        assert!(total >= 2, "should rename both definition and reference");
    }

    #[test]
    fn test_rename_signal_on_definition() {
        let html = r#"<div data-signals:counter="0"><span data-text="$counter"></span></div>"#;
        let result = ren_for(html, ":counter", "count");
        assert!(result.is_some(), "should rename from definition position");
    }

    #[test]
    fn test_rename_undefined_signal() {
        let result = ren_for(r#"<div data-text="$foo"></div>"#, "$foo", "bar");
        assert!(result.is_none(), "undefined signal should not rename");
    }

    #[test]
    fn test_rename_invalid_name() {
        let html = r#"<div data-signals:counter="0"><span data-text="$counter"></span></div>"#;
        let result = ren_for(html, "$counter", "my name");
        assert!(result.is_none(), "invalid name should fail");
    }
}
