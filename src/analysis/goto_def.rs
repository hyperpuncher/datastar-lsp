use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Position, Range, Url};

use crate::analysis::project_index::ProjectIndex;
use crate::analysis::signals::analyze_signals;
use crate::parser::html::DataAttribute;

/// Find the definition location of a signal or action at the given position.
/// Searches local document first, then cross-file index.
pub fn goto_definition(
    text: &str,
    position: Position,
    uri: &Url,
    attrs: &[DataAttribute],
    project_index: Option<&ProjectIndex>,
) -> Option<GotoDefinitionResponse> {
    let offset = crate::util::position_to_byte_offset(text, position);

    // Check if cursor is inside a signal reference in an attribute value
    for attr in attrs {
        let value = match attr.value.as_ref() {
            Some(v) => v,
            None => continue,
        };
        let value_start = match attr.value_start {
            Some(s) => s,
            None => continue,
        };
        let value_end = value_start + value.len() + 2;

        if offset < value_start || offset > value_end {
            continue;
        }

        let rel_offset = offset - value_start - 1;
        if rel_offset >= value.len() {
            continue;
        }

        let bytes = value.as_bytes();

        // Check for $signal reference
        if bytes[rel_offset] == b'$' {
            // Extract full signal name starting here
            let name_start = rel_offset + 1;
            let mut name_end = name_start;
            while name_end < bytes.len()
                && (bytes[name_end].is_ascii_alphanumeric()
                    || bytes[name_end] == b'-'
                    || bytes[name_end] == b'_'
                    || bytes[name_end] == b'.'
                    || bytes[name_end] == b'['
                    || bytes[name_end] == b']')
            {
                name_end += 1;
            }
            let raw = std::str::from_utf8(&bytes[name_start..name_end]).unwrap_or("");
            let signal_ref = raw
                .trim_end_matches("++")
                .trim_end_matches("--")
                .trim_end_matches('+')
                .trim_end_matches('-')
                .trim_end_matches('.');
            let top_name = signal_ref.split('.').next().unwrap_or("");
            return find_signal_definition(text, top_name, uri, project_index);
        }

        // Check if cursor is inside a signal name (after $, not at $ itself)
        if let Some(signal_name) = find_signal_name_at(value, rel_offset) {
            let top_name = signal_name.split('.').next().unwrap_or("");
            return find_signal_definition(text, top_name, uri, project_index);
        }
    }

    None
}

/// Scan backward from offset to see if we're inside a $signal reference.
fn find_signal_name_at(value: &str, offset: usize) -> Option<String> {
    let bytes = value.as_bytes();
    if offset >= bytes.len() {
        return None;
    }

    // Scan backward to find $
    let mut start = offset;
    loop {
        if bytes[start] == b'$' {
            let name_start = start + 1;
            let mut name_end = name_start;
            while name_end < bytes.len()
                && (bytes[name_end].is_ascii_alphanumeric()
                    || bytes[name_end] == b'-'
                    || bytes[name_end] == b'_'
                    || bytes[name_end] == b'.'
                    || bytes[name_end] == b'['
                    || bytes[name_end] == b']')
            {
                name_end += 1;
            }
            if name_end > name_start && offset < name_end {
                let raw = std::str::from_utf8(&bytes[name_start..name_end]).unwrap_or("");
                let trimmed = raw
                    .trim_end_matches("++")
                    .trim_end_matches("--")
                    .trim_end_matches('+')
                    .trim_end_matches('-')
                    .trim_end_matches('.');
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            return None;
        }
        if start == 0 {
            break;
        }
        // Stop if we hit a non-signal-name char
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

/// Find the definition of a top-level signal name in the document.
/// Falls back to cross-file project index if not found locally.
fn find_signal_definition(
    text: &str,
    top_name: &str,
    uri: &Url,
    project_index: Option<&ProjectIndex>,
) -> Option<GotoDefinitionResponse> {
    let analysis = analyze_signals(text);

    // Try local first
    if let Some(defs) = analysis.definitions.get(top_name) {
        let def = defs.first()?;
        let pos = crate::util::byte_to_position(text, def.byte_offset);
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
            if let Some(entry) = index.documents.get(cross_uri) {
                let (doc_text, _) = &*entry;
                let pos = crate::util::byte_to_position(doc_text, *byte_offset);
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

    #[test]
    fn test_goto_signal_definition() {
        let html = r#"<div data-signals:foo="1"><span data-text="$foo"></span></div>"#;
        let parsed = crate::parser::html::parse_html(html.as_bytes()).unwrap();
        let uri = Url::parse("file:///test.html").unwrap();

        // Cursor at $foo in data-text
        let dollar_pos = html.find("$foo").unwrap();
        let pos = Position {
            line: 0,
            character: dollar_pos as u32 + 1, // inside "foo"
        };

        let result = goto_definition(html, pos, &uri, &parsed.1, None);
        assert!(result.is_some(), "should find definition for $foo");

        if let GotoDefinitionResponse::Scalar(loc) = result.unwrap() {
            // Should point to data-signals:foo attribute
            let line_text = &html[loc.range.start.character as usize..];
            assert!(
                line_text.contains("data-signals") || line_text.contains("foo"),
                "should navigate to signal definition, got: {}",
                line_text
            );
        }
    }

    #[test]
    fn test_goto_undefined_signal() {
        let html = r#"<div data-text="$nonexistent"></div>"#;
        let parsed = crate::parser::html::parse_html(html.as_bytes()).unwrap();
        let uri = Url::parse("file:///test.html").unwrap();

        let dollar_pos = html.find("$nonexistent").unwrap();
        let pos = Position {
            line: 0,
            character: dollar_pos as u32 + 1,
        };

        let result = goto_definition(html, pos, &uri, &parsed.1, None);
        assert!(
            result.is_none(),
            "undefined signal should have no definition"
        );
    }

    #[test]
    fn test_goto_bind_definition() {
        let html = r#"<input data-bind:count /><button data-on:click="$count++">+</button>"#;
        let parsed = crate::parser::html::parse_html(html.as_bytes()).unwrap();
        let uri = Url::parse("file:///test.html").unwrap();

        let dollar_pos = html.find("$count").unwrap();
        let pos = Position {
            line: 0,
            character: dollar_pos as u32 + 1,
        };

        let result = goto_definition(html, pos, &uri, &parsed.1, None);
        assert!(result.is_some(), "should find definition for $count");
    }
}
