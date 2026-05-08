use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::analysis::project_index::ProjectIndex;
use crate::analysis::signals::analyze_signals;

/// Find all references to the signal or action at the given position.
/// Searches local document first, then cross-file index.
pub fn find_references(
    text: &str,
    position: Position,
    uri: &Url,
    project_index: Option<&ProjectIndex>,
) -> Vec<Location> {
    let offset = super::completions::position_to_byte_offset(text, position);
    let analysis = analyze_signals(text);
    let bytes = text.as_bytes();

    // Find which signal reference the cursor is on
    let cursor_name = find_signal_name_at_offset(bytes, offset);

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
            let pos = byte_to_position(text, def.byte_offset);
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
            let pos = byte_to_position(text, ref_.byte_offset);
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
            if let Some(entry) = index.documents.get(&cross_uri) {
                let (doc_text, _) = &*entry;
                let pos = byte_to_position(doc_text, byte_offset);
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

/// Check if cursor is on a $signal reference. Returns the full signal name (e.g. "foo.bar").
fn find_signal_name_at_offset(bytes: &[u8], offset: usize) -> Option<String> {
    if offset >= bytes.len() {
        return None;
    }

    // If cursor is directly on $, look ahead
    if bytes[offset] == b'$' {
        return extract_signal_name_forward(bytes, offset + 1);
    }

    // Scan backward from cursor to find the $
    let mut start = offset;
    loop {
        if bytes[start] == b'$' {
            if offset >= start + 1 {
                // Cursor is after $ — extract from $ onward
                let full = extract_signal_name_forward(bytes, start + 1);
                if let Some(_name) = &full {
                    let cursor_rel = offset - (start + 1);
                    let len = std::str::from_utf8(&bytes[start + 1..]).unwrap_or("").len();
                    if cursor_rel < len {
                        return full;
                    }
                }
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
            return None;
        }
        start -= 1;
    }
    None
}

/// Extract signal name starting at offset (after $).
fn extract_signal_name_forward(bytes: &[u8], start: usize) -> Option<String> {
    if start >= bytes.len() {
        return None;
    }
    let mut end = start;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric()
            || bytes[end] == b'-'
            || bytes[end] == b'_'
            || bytes[end] == b'.'
            || bytes[end] == b'['
            || bytes[end] == b']')
    {
        end += 1;
    }
    if end <= start {
        return None;
    }
    let raw = std::str::from_utf8(&bytes[start..end]).unwrap_or("");
    // Trim postfix operators
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

    fn loc_line(locs: &[Location]) -> Vec<u32> {
        let mut lines: Vec<_> = locs.iter().map(|l| l.range.start.line).collect();
        lines.sort();
        lines.dedup();
        lines
    }

    #[test]
    fn test_find_references() {
        let html = r#"<div data-signals:counter="0"><span data-text="$counter"></span><button data-on:click="$counter++">+</button></div>"#;
        let uri = Url::parse("file:///test.html").unwrap();

        let dollar_pos = html.find("$counter").unwrap();
        let pos = Position {
            line: 0,
            character: dollar_pos as u32 + 1,
        };

        let locs = find_references(html, pos, &uri, None);
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

        let locs = find_references(html, pos, &uri, None);
        assert!(
            locs.is_empty(),
            "undefined signal should have no references"
        );
    }
}
