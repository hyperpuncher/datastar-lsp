use crate::analysis::ts_util::AttrData;
use crate::analysis::value_scanner;

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
    "data-match-media:",
];

/// Return true if the top-level signal name is a builtin (evt, el, __*).
pub fn is_builtin_signal(top: &str) -> bool {
    top == "evt" || top == "el" || top.starts_with("__")
}

/// Global modifiers that work on any attribute.
pub const GLOBAL_MODIFIERS: &[&str] = &["case", "delay", "viewtransition"];

/// Check if a modifier key is global (works on any attribute).
pub fn is_global_modifier(key: &str) -> bool {
    GLOBAL_MODIFIERS.contains(&key)
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
        if let Some(name) = value_scanner::signal_at_cursor(value, rel) {
            return Some(name);
        }
    }
    None
}

/// Check if a signal name is defined locally (attrs) or across the project index.
pub fn is_defined(
    top: &str,
    attrs: &[AttrData],
    project_index: Option<&crate::analysis::project_index::ProjectIndex>,
) -> bool {
    attrs
        .iter()
        .filter(|a| DEFINERS.contains(&a.plugin_name.as_str()))
        .any(|a| a.key.as_deref() == Some(top))
        || project_index
            .as_ref()
            .is_some_and(|idx| index_find_def(idx, top))
}

/// Check if a signal name is found in cross-file index text.
pub fn index_find_def(index: &crate::analysis::project_index::ProjectIndex, name: &str) -> bool {
    index.iter().any(|e| {
        let li = e.value();
        let t = li.text();
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
    fn test_is_valid_signal_name() {
        assert!(is_valid_signal_name("counter"));
        assert!(is_valid_signal_name("user-name"));
        assert!(is_valid_signal_name("my_signal"));
        assert!(!is_valid_signal_name(""));
        assert!(!is_valid_signal_name("my name"));
    }
}
