use tower_lsp::lsp_types::Position;

/// Convert byte offset to LSP Position (line, character).
pub fn byte_to_position(text: &str, byte_offset: usize) -> Position {
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

/// Convert LSP Position to byte offset in text.
pub fn position_to_byte_offset(text: &str, pos: Position) -> usize {
    let mut line = 0u32;

    for (i, c) in text.char_indices() {
        if line == pos.line {
            let mut char_count = 0u32;
            for (j, _ch) in text[i..].char_indices() {
                if char_count >= pos.character {
                    return i + j;
                }
                char_count += 1;
            }
            return text.len();
        }
        if c == '\n' {
            line += 1;
        }
    }

    text.len()
}

/// Convert byte range to LSP Range.
pub fn byte_range_to_lsp_range(
    text: &str,
    start_byte: usize,
    end_byte: usize,
) -> tower_lsp::lsp_types::Range {
    let start = byte_to_position(text, start_byte.min(text.len()));
    let end = byte_to_position(text, end_byte.min(text.len()));
    tower_lsp::lsp_types::Range { start, end }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_to_byte_offset() {
        let text = "line1\nline2\nline3";
        let pos = position_to_byte_offset(
            text,
            Position {
                line: 1,
                character: 2,
            },
        );
        assert_eq!(pos, 8);
        assert_eq!(&text[pos..pos + 1], "n");
    }

    #[test]
    fn test_byte_to_position() {
        let text = "ab\ncd";
        let pos = byte_to_position(text, 3);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
    }
}
