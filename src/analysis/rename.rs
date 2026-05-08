use tower_lsp::lsp_types::{Position, TextEdit, WorkspaceEdit};

use crate::analysis::project_index::ProjectIndex;
use crate::analysis::signals::analyze_signals;

/// Produce workspace edits to rename a signal across all open documents.
/// Returns None if the cursor is not on a defined signal reference or definition.
pub fn rename_signal(
    text: &str,
    position: Position,
    new_name: &str,
    project_index: Option<&ProjectIndex>,
) -> Option<WorkspaceEdit> {
    let offset = super::completions::position_to_byte_offset(text, position);
    let analysis = analyze_signals(text);
    let bytes = text.as_bytes();

    // Validate new_name is a valid signal identifier
    if !is_valid_signal_name(new_name) {
        return None;
    }

    // Find which signal the cursor is on
    let cursor_name = find_signal_name_at(bytes, offset);
    let old_name = match cursor_name {
        Some(ref name) => name.split('.').next().unwrap_or("").to_string(),
        None => return None,
    };

    if old_name.is_empty() {
        return None;
    }

    if !analysis.top_level_names.contains(&old_name) {
        return None;
    }

    let mut edits = Vec::new();

    // Rename in definitions
    if let Some(defs) = analysis.definitions.get(&old_name) {
        for def in defs {
            // Replace old top-level name with new_name
            // For signals:counter → signals:newname
            // Need to scan for colon+name pattern near definition
            if let Some(edit) = make_definition_edit(text, &def, &old_name, new_name) {
                edits.push(edit);
            }
        }
    }

    // Rename in references: $old_name, $old_name++, $old_name.bar
    for ref_ in &analysis.references {
        let ref_top = ref_.name.split('.').next().unwrap_or("");
        if ref_top == old_name {
            let start = byte_to_position(text, ref_.byte_offset);
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

    // Cross-file rename: find references in other open documents
    if let Some(index) = project_index {
        for (cross_uri, byte_offset, len) in index.find_all_references(&old_name) {
            if let Some(entry) = index.documents.get(&cross_uri) {
                let (doc_text, _) = &*entry;
                let pos = byte_to_position(doc_text, byte_offset);
                edits.push(TextEdit {
                    range: tower_lsp::lsp_types::Range {
                        start: pos,
                        end: Position {
                            line: pos.line,
                            character: pos.character + len as u32,
                        },
                    },
                    new_text: format!("${}", new_name),
                });
            }
        }
    }

    if edits.is_empty() {
        return None;
    }

    Some(WorkspaceEdit {
        changes: Some(
            [(
                tower_lsp::lsp_types::Url::parse("file:///dummy").expect("valid url"),
                edits,
            )]
            .into_iter()
            .collect(),
        ),
        ..Default::default()
    })
}

/// Create a TextEdit for renaming the signal name in a definition.
fn make_definition_edit(
    text: &str,
    def: &crate::analysis::signals::SignalDef,
    old_name: &str,
    new_name: &str,
) -> Option<TextEdit> {
    let bytes = text.as_bytes();
    let def_offset = def.byte_offset;

    // The definition name appears after "data-SOMETHING:name" pattern
    // Find the colon after the plugin name, then the name
    let after_data = &bytes[def_offset + 5..];
    let colon_pos = after_data.iter().position(|&b| b == b':')?;
    let name_start_relative = colon_pos + 1;
    let name_start = def_offset + 5 + name_start_relative;

    // The name might be followed by modifier (_key) or attribute end (=' ')
    let name_bytes = &bytes[name_start..];
    let mut name_len = 0usize;
    while name_len < name_bytes.len()
        && (name_bytes[name_len].is_ascii_alphanumeric()
            || name_bytes[name_len] == b'-'
            || name_bytes[name_len] == b'_')
    {
        name_len += 1;
    }

    if name_len == 0 {
        return None;
    }

    let actual_name = std::str::from_utf8(&name_bytes[..name_len]).unwrap_or("");
    if actual_name != old_name {
        // This definition is for a compound signal like foo.bar — skip
        return None;
    }

    let start_pos = byte_to_position(text, name_start);
    let end_pos = Position {
        line: start_pos.line,
        character: start_pos.character + name_len as u32,
    };

    Some(TextEdit {
        range: tower_lsp::lsp_types::Range {
            start: start_pos,
            end: end_pos,
        },
        new_text: new_name.to_string(),
    })
}

/// Find signal name at cursor position in raw bytes.
fn find_signal_name_at(bytes: &[u8], offset: usize) -> Option<String> {
    if offset >= bytes.len() {
        return None;
    }

    // If directly on $, look forward
    if bytes[offset] == b'$' {
        return extract_signal_name_forward(bytes, offset + 1);
    }

    // Scan backward for $
    let mut start = offset;
    loop {
        if bytes[start] == b'$' {
            if offset > start {
                return extract_signal_name_forward(bytes, start + 1);
            }
            return None;
        }
        if start == 0 {
            break;
        }
        if !(bytes[start].is_ascii_alphanumeric()
            || bytes[start] == b'-'
            || bytes[start] == b'_'
            || bytes[start] == b'.'
            || bytes[start] == b'['
            || bytes[start] == b']')
        {
            // Check if we're on a definition attribute name (data-signals:FOO)
            return find_definition_name_at(bytes, offset);
        }
        start -= 1;
    }

    // If no $ found, check if cursor is on a definition name
    find_definition_name_at(bytes, offset)
}

/// Check if cursor is on a signal definition name (after data-signals:FOO).
fn find_definition_name_at(bytes: &[u8], offset: usize) -> Option<String> {
    // Scan backward from offset to find ":"
    let mut i = offset;
    while i > 0 {
        if bytes[i] == b':' {
            // Check what's before the colon — data-signals, data-bind, etc.
            let before = &bytes[..i];
            let before_str = match std::str::from_utf8(before) {
                Ok(s) => s,
                Err(_) => return None,
            };

            let is_definer = before_str.ends_with("data-signals")
                || before_str.ends_with("data-bind")
                || before_str.ends_with("data-computed")
                || before_str.ends_with("data-ref")
                || before_str.ends_with("data-indicator");
            if is_definer {
                // Extract name after ":"
                let name = extract_signal_name_forward(bytes, i + 1);
                if let Some(ref n) = name {
                    if offset > i && offset <= i + 1 + n.len() {
                        return name;
                    }
                }
            }
            return None;
        }
        i -= 1;
    }
    None
}

/// Extract signal name bytes starting at offset (after $ or after :).
fn extract_signal_name_forward(bytes: &[u8], start: usize) -> Option<String> {
    if start >= bytes.len() {
        return None;
    }

    let first = bytes[start];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }

    let mut end = start;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric()
            || bytes[end] == b'-'
            || bytes[end] == b'_'
            || bytes[end] == b'.')
    {
        end += 1;
    }

    if end <= start {
        return None;
    }

    let raw = std::str::from_utf8(&bytes[start..end]).unwrap_or("");
    let trimmed = raw
        .trim_end_matches("++")
        .trim_end_matches("--")
        .trim_end_matches('+')
        .trim_end_matches('-')
        .trim_end_matches('.');
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn is_valid_signal_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn byte_to_position(text: &str, byte_offset: usize) -> Position {
    let byte_offset = byte_offset.min(text.len());
    let mut line = 0u32;
    let mut col = 0u32;

    for (i, c) in text.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += c.len_utf8() as u32;
        }
    }

    Position {
        line,
        character: col,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rename_signal_on_reference() {
        let html = r#"<div data-signals:counter="0"><span data-text="$counter"></span></div>"#;
        let dollar_pos = html.find("$counter").unwrap();
        let pos = Position {
            line: 0,
            character: dollar_pos as u32 + 2,
        };

        let result = rename_signal(html, pos, "count", None);
        assert!(result.is_some(), "should produce workspace edit");

        let edit = result.unwrap();
        let changes = edit.changes.unwrap();
        let edits = changes.values().next().unwrap();
        assert!(
            edits.len() >= 2,
            "should rename both definition and reference"
        );
    }

    #[test]
    fn test_rename_signal_on_definition() {
        let html = r#"<div data-signals:counter="0"><span data-text="$counter"></span></div>"#;
        let def_pos = html.find(":counter").unwrap() + 1;
        let pos = Position {
            line: 0,
            character: def_pos as u32,
        };

        let result = rename_signal(html, pos, "count", None);
        assert!(result.is_some(), "should rename from definition position");
    }

    #[test]
    fn test_rename_undefined_signal() {
        let html = r#"<div data-text="$foo"></div>"#;
        let dollar_pos = html.find("$foo").unwrap();
        let pos = Position {
            line: 0,
            character: dollar_pos as u32 + 1,
        };

        let result = rename_signal(html, pos, "bar", None);
        assert!(result.is_none(), "undefined signal should not rename");
    }

    #[test]
    fn test_rename_invalid_name() {
        let html = r#"<div data-signals:counter="0"><span data-text="$counter"></span></div>"#;
        let dollar_pos = html.find("$counter").unwrap();
        let pos = Position {
            line: 0,
            character: dollar_pos as u32 + 1,
        };

        let result = rename_signal(html, pos, "my name", None);
        assert!(result.is_none(), "invalid name should fail");
    }
}
