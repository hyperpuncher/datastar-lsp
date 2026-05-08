use tower_lsp::lsp_types::Position;

/// Convert byte range to LSP Range using pre-computed line index.
/// Cheap: O(log n) line lookup via binary search.
pub fn byte_range_to_lsp_range(
    line_index: &crate::line_index::LineIndex,
    start_byte: usize,
    end_byte: usize,
) -> tower_lsp::lsp_types::Range {
    let text_len = line_index.text().len();
    let (start_line, start_char) = line_index.byte_to_position(start_byte.min(text_len));
    let (end_line, end_char) = line_index.byte_to_position(end_byte.min(text_len));
    tower_lsp::lsp_types::Range {
        start: Position {
            line: start_line,
            character: start_char,
        },
        end: Position {
            line: end_line,
            character: end_char,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_range_to_lsp_range() {
        let idx = crate::line_index::LineIndex::new("ab\ncd".to_string());
        let r = byte_range_to_lsp_range(&idx, 0, 1);
        assert_eq!(r.start.line, 0);
        assert_eq!(r.start.character, 0);
        assert_eq!(r.end.line, 0);
        assert_eq!(r.end.character, 1);

        let r2 = byte_range_to_lsp_range(&idx, 3, 4);
        assert_eq!(r2.start.line, 1);
        assert_eq!(r2.start.character, 0);
    }
}
