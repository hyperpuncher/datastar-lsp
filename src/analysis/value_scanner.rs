/// Shared byte-level scanner for Datastar attribute values.
///
/// Scans for `$signal`, `@action`, and `evt.` token spans.
/// Consolidates 4+ duplicated scanning implementations across the codebase.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueSpan {
    pub kind: SpanKind,
    /// The identifier: signal name, action name, or event property suffix.
    pub name: String,
    /// Byte offset within the value string (relative to value start).
    pub start: usize,
    /// Byte offset within the value string (exclusive).
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    /// `$counter` or `$user.name`
    DollarSignal,
    /// `@get` or `@post`
    AtAction,
    /// `evt.key` or `evt.clientX`
    EvtDotProp,
}

/// Scan a value for all Datastar tokens. The result can be filtered by kind.
pub fn scan_value(value: &str) -> Vec<ValueSpan> {
    let mut spans = Vec::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'$' => {
                let end = if let Some(span) = scan_id(value, bytes, i) {
                    let e = span.end;
                    spans.push(span);
                    e
                } else {
                    i + 1
                };
                i = end;
                continue;
            }
            b'@' => {
                if let Some(span) = scan_id(value, bytes, i) {
                    let end = span.end;
                    spans.push(ValueSpan {
                        kind: SpanKind::AtAction,
                        ..span
                    });
                    i = end;
                    continue;
                }
            }
            b'e' if value[i..].starts_with("evt.") => {
                let start = i;
                i += 4; // skip "evt."
                let prop_start = i;
                while i < bytes.len() && is_id_char(bytes[i]) {
                    i += 1;
                }
                if i > prop_start {
                    let name = value[prop_start..i].to_string();
                    spans.push(ValueSpan {
                        kind: SpanKind::EvtDotProp,
                        name,
                        start,
                        end: i,
                    });
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    spans
}

/// Find the span containing a byte offset within the value.
pub fn span_at(value: &str, rel_byte_offset: usize) -> Option<ValueSpan> {
    scan_value(value)
        .into_iter()
        .find(|s| rel_byte_offset >= s.start && rel_byte_offset < s.end)
}

/// Find the signal name at a cursor position, backtracking through `$name.path`.
pub fn signal_at_cursor(value: &str, rel: usize) -> Option<String> {
    let bytes = value.as_bytes();
    if rel > bytes.len() {
        return None;
    }
    // Direct hit on `$` — cursor at position 0 on the dollar sign
    if bytes.get(rel) == Some(&b'$') {
        return read_signal(&value[rel + 1..]);
    }
    // Cursor on identifier chars — backtrack to find `$`
    if rel < bytes.len() && is_id_char(bytes[rel]) {
        let mut start = rel;
        while start > 0 {
            let c = bytes[start - 1];
            if is_id_char(c) {
                start -= 1;
            } else {
                break;
            }
        }
        if start > 0 && bytes[start - 1] == b'$' {
            return read_signal(&value[start..]);
        }
    }
    // Check if cursor is just past end of signal — backtrack one char
    if rel > 0 && is_id_char(bytes[rel - 1]) {
        let mut start = rel - 1;
        while start > 0 {
            let c = bytes[start - 1];
            if is_id_char(c) {
                start -= 1;
            } else {
                break;
            }
        }
        if start > 0 && bytes[start - 1] == b'$' {
            return read_signal(&value[start..]);
        }
    }
    // Fallback: scan and check
    for span in scan_value(value) {
        if span.kind == SpanKind::DollarSignal && rel >= span.start && rel <= span.end {
            return Some(span.name);
        }
    }
    None
}

/// Scan identifiers starting at `pos` (right after `$` or `@`).
fn scan_id(value: &str, bytes: &[u8], pos: usize) -> Option<ValueSpan> {
    if pos + 1 >= bytes.len() {
        return None;
    }
    let next = bytes[pos + 1];
    if !next.is_ascii_alphabetic() && next != b'_' {
        return None;
    }
    let mut end = pos + 1;
    while end < bytes.len() && is_id_char(bytes[end]) {
        end += 1;
    }
    if end <= pos + 1 {
        return None;
    }
    let raw = &value[pos + 1..end];
    let name = raw
        .trim_end_matches("++")
        .trim_end_matches("--")
        .trim_end_matches('.');
    if name.is_empty() {
        return None;
    }
    Some(ValueSpan {
        kind: SpanKind::DollarSignal,
        name: name.to_string(),
        start: pos,
        end,
    })
}

/// Read a signal name from text starting right after `$`.
/// Trims `++`/`--`/`.` postfixes. Returns None if empty.
fn read_signal(after: &str) -> Option<String> {
    let end = after
        .find(|c: char| !is_id_char(c as u8))
        .unwrap_or(after.len());
    let raw = &after[..end];
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

fn is_id_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'.'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_dollar_signal() {
        let spans = scan_value("$counter + $user.name > 0");
        let sigs: Vec<_> = spans
            .iter()
            .filter(|s| s.kind == SpanKind::DollarSignal)
            .map(|s| &s.name)
            .collect();
        assert_eq!(sigs, vec!["counter", "user.name"]);
    }

    #[test]
    fn test_scan_postfix_signal() {
        let spans = scan_value("$counter++");
        assert_eq!(spans[0].name, "counter");
    }

    #[test]
    fn test_scan_action() {
        let spans = scan_value("@get('/api') + @post('/submit')");
        let acts: Vec<_> = spans
            .iter()
            .filter(|s| s.kind == SpanKind::AtAction)
            .map(|s| &s.name)
            .collect();
        assert_eq!(acts, vec!["get", "post"]);
    }

    #[test]
    fn test_scan_evt_dot() {
        let spans = scan_value("evt.key + evt.clientX > 10");
        let evt: Vec<_> = spans
            .iter()
            .filter(|s| s.kind == SpanKind::EvtDotProp)
            .map(|s| &s.name)
            .collect();
        assert_eq!(evt, vec!["key", "clientX"]);
    }

    #[test]
    fn test_span_at() {
        let span = span_at("$counter", 1).unwrap();
        assert_eq!(span.kind, SpanKind::DollarSignal);
        assert_eq!(span.name, "counter");
    }

    #[test]
    fn test_span_at_none() {
        assert!(span_at("hello", 2).is_none());
    }

    #[test]
    fn test_signal_at_cursor_on_dollar() {
        assert_eq!(
            signal_at_cursor("$counter", 0),
            Some("counter".to_string())
        );
    }

    #[test]
    fn test_signal_at_cursor_on_char() {
        assert_eq!(
            signal_at_cursor("$counter", 3),
            Some("counter".to_string())
        );
    }

    #[test]
    fn test_signal_at_cursor_backtrack() {
        assert_eq!(
            signal_at_cursor("$user.name", 6),
            Some("user.name".to_string())
        );
    }

    #[test]
    fn test_signal_at_cursor_postfix() {
        assert_eq!(
            signal_at_cursor("$counter++", 8),
            Some("counter".to_string())
        );
    }
}
