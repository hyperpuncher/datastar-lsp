/// Pre-computed line starts for fast byte↔position conversion.
///
/// The LSP uses **character** offsets (Unicode char count, not byte count).
/// This index finds the correct line in O(log n) then uses the original
/// character-scanning logic within that line only.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of the start of each line.
    line_starts: Vec<usize>,
    /// Full text for character-level conversion.
    text: String,
}

impl LineIndex {
    pub fn new(text: String) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts, text }
    }

    /// Convert byte offset to LSP Position (line, character).
    pub fn byte_to_position(&self, byte_offset: usize) -> (u32, u32) {
        let byte_offset = byte_offset.min(self.text.len());
        let line = match self.line_starts.binary_search(&byte_offset) {
            Ok(exact) => exact,
            Err(insertion) => insertion.saturating_sub(1),
        };
        let line_start = self.line_starts[line];

        // Count characters from line start to byte offset
        let char_count = self.text[line_start..byte_offset].chars().count();
        (line as u32, char_count as u32)
    }

    /// Convert LSP Position to byte offset.
    pub fn position_to_byte_offset(&self, line: u32, col: u32) -> usize {
        let line = line as usize;
        if line >= self.line_starts.len() {
            return self.text.len();
        }
        let line_start = self.line_starts[line];
        let line_end = self
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.text.len());

        // Count characters from line start to col
        let mut byte_offset = line_start;
        let mut char_count = 0u32;
        let mut last_char_len = 0usize;
        for (i, c) in self.text[line_start..line_end].char_indices() {
            if char_count >= col {
                break;
            }
            byte_offset = line_start + i;
            last_char_len = c.len_utf8();
            char_count += 1;
        }
        if char_count == col && char_count > 0 {
            byte_offset += last_char_len;
        }
        if char_count < col {
            byte_offset = line_end;
        }

        byte_offset
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        let idx = LineIndex::new("ab\ncd\nef".to_string());
        assert_eq!(idx.byte_to_position(0), (0, 0));
        assert_eq!(idx.byte_to_position(1), (0, 1));
        assert_eq!(idx.byte_to_position(3), (1, 0));
        assert_eq!(idx.byte_to_position(6), (2, 0));
        assert_eq!(idx.byte_to_position(8), (2, 2));

        assert_eq!(idx.position_to_byte_offset(0, 0), 0);
        assert_eq!(idx.position_to_byte_offset(1, 0), 3);
        assert_eq!(idx.position_to_byte_offset(2, 2), 8);
    }

    #[test]
    fn test_multibyte() {
        let idx = LineIndex::new("héllo\nwörld".to_string());
        // h(0) é(1-2) l(3) l(4) o(5) \n(6) w(7) ö(8-9) r(10) l(11) d(12)
        assert_eq!(idx.byte_to_position(0), (0, 0)); // h
        assert_eq!(idx.byte_to_position(1), (0, 1)); // é (2 bytes, 1 char)
        assert_eq!(idx.byte_to_position(3), (0, 2)); // l (after 2-char é)
        assert_eq!(idx.byte_to_position(7), (1, 0)); // w

        assert_eq!(idx.position_to_byte_offset(0, 0), 0);
        assert_eq!(idx.position_to_byte_offset(0, 1), 1); // é starts at byte 1
        assert_eq!(idx.position_to_byte_offset(0, 2), 3); // l starts at byte 3
        assert_eq!(idx.position_to_byte_offset(1, 0), 7);
    }
}
