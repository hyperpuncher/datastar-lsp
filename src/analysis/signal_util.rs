use crate::analysis::ts_util::AttrData;

/// Attribute plugin names that define signals.
pub const DEFINERS: &[&str] = &[
    "signals",
    "bind",
    "computed",
    "ref",
    "indicator",
    "match-media",
];

/// Full `data-{prefix}:` strings for cross-file string scanning.
pub const DEFINER_PREFIXES: &[&str] = &[
    "data-signals:",
    "data-bind:",
    "data-computed:",
    "data-ref:",
    "data-indicator:",
];

/// Return true if the top-level signal name is a builtin (evt, el, __*).
pub fn is_builtin_signal(top: &str) -> bool {
    top == "evt" || top == "el" || top.starts_with("__")
}

/// Global modifiers that work on any attribute.
pub fn is_global_modifier(key: &str) -> bool {
    matches!(key, "case" | "delay" | "viewtransition")
}

/// Scan a value string for `$signal` references, trimming `++`/`--`/`.` postfixes.
pub fn scan_signals(value: &str) -> Vec<String> {
    let mut results = Vec::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next.is_ascii_alphabetic() || next == b'_' {
                let start = i + 1;
                let mut j = start;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric()
                        || bytes[j] == b'_'
                        || bytes[j] == b'-'
                        || bytes[j] == b'.')
                {
                    j += 1;
                }
                if j > start {
                    let raw = std::str::from_utf8(&bytes[start..j]).unwrap_or("");
                    let trimmed = raw
                        .trim_end_matches("++")
                        .trim_end_matches("--")
                        .trim_end_matches('.');
                    if !trimmed.is_empty() {
                        results.push(trimmed.to_string());
                    }
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    results
}

/// Read a signal name starting after `$`. Trims `++`/`--`/`.` postfixes.
pub fn read_signal_name(after_dollar: &str) -> Option<String> {
    let end = after_dollar
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-' && c != '.')
        .unwrap_or(after_dollar.len());
    let raw = &after_dollar[..end];
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

/// Find the signal name at a cursor byte offset within attribute values.
pub fn find_signal_at_cursor(attrs: &[AttrData], offset: usize) -> Option<String> {
    for attr in attrs {
        let (Some(value_start), Some(value)) = (attr.value_start, &attr.value) else {
            continue;
        };
        let value_end = value_start + value.len();
        if offset < value_start || offset > value_end {
            continue;
        }
        let rel = offset.saturating_sub(value_start);
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

/// Check if a signal name is defined in a set of definition attr keys.
pub fn is_defined(
    top: &str,
    attrs: &[AttrData],
    project_index: Option<&crate::analysis::project_index::ProjectIndex>,
) -> bool {
    attrs
        .iter()
        .filter(|a| DEFINERS.contains(&a.plugin_name.as_str()))
        .any(|a| a.key.as_deref() == Some(top))
        || project_index.as_ref().is_some_and(|idx| {
            idx.iter().any(|e| {
                let (_li, t) = e.value();
                t.contains(&format!("data-signals:{top}"))
                    || t.contains(&format!("data-bind:{top}"))
            })
        })
}

/// Check if a signal name is found in cross-file index text.
pub fn index_find_def(index: &crate::analysis::project_index::ProjectIndex, name: &str) -> bool {
    index.iter().any(|e| {
        let (_li, t) = e.value();
        DEFINER_PREFIXES
            .iter()
            .any(|prefix| t.contains(&format!("{prefix}{name}")))
    })
}

/// Validate that a signal name is legal (for rename operations).
pub fn is_valid_signal_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_signal_name_basic() {
        assert_eq!(read_signal_name("counter"), Some("counter".into()));
    }

    #[test]
    fn test_read_signal_name_with_postfix() {
        assert_eq!(read_signal_name("counter++"), Some("counter".into()));
        assert_eq!(read_signal_name("counter--"), Some("counter".into()));
    }

    #[test]
    fn test_read_signal_name_dotted() {
        assert_eq!(read_signal_name("user.name"), Some("user.name".into()));
    }

    #[test]
    fn test_scan_signals() {
        let signals = scan_signals("$counter + $user.name > 0");
        assert_eq!(signals, vec!["counter", "user.name"]);
    }

    #[test]
    fn test_scan_signals_with_postfix() {
        let signals = scan_signals("$counter++");
        assert_eq!(signals, vec!["counter"]);
    }
}
