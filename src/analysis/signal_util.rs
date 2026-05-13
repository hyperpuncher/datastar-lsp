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
        .any(|a| signal_names_from_attr(a).contains(&top.to_string()))
        || project_index
            .as_ref()
            .is_some_and(|idx| index_find_def(idx, top))
}

/// Extract all signal names that an attribute defines.
/// Handles key-based (data-bind:foo), value-based (data-bind="foo"),
/// and object-literal-based (data-signals="{foo: 1, bar: 2}").
pub fn signal_names_from_attr(attr: &AttrData) -> Vec<String> {
    let mut names = Vec::new();

    // Key-based: data-bind:foo → signal is "foo"
    if let Some(ref k) = attr.key {
        names.push(k.clone());
        return names;
    }

    let Some(ref value) = attr.value else {
        return names;
    };

    let trimmed = value.trim();

    // Object literal: data-signals="{foo: 1, bar: 2}"
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with("{\"") && trimmed.ends_with("\"}"))
    {
        let inner = if trimmed.starts_with("{\"") {
            &trimmed[2..trimmed.len() - 2]
        } else {
            &trimmed[1..trimmed.len() - 1]
        };
        for part in split_obj_keys(inner) {
            let name = part.trim();
            if !name.is_empty() && is_valid_signal_name(name) {
                names.push(name.to_string());
            }
        }
        return names;
    }

    // Simple value: data-bind="foo", data-ref="bar"
    if !trimmed.is_empty() && is_valid_signal_name(trimmed) {
        names.push(trimmed.to_string());
    }

    names
}

/// Split object literal content on top-level commas,
/// extracting key names before `:`.
fn split_obj_keys(content: &str) -> Vec<&str> {
    let mut keys = Vec::new();
    let mut depth = 0u32;
    let mut last = 0;
    for (i, c) in content.char_indices() {
        match c {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let part = content[last..i].trim();
                if let Some(key) = extract_key(part) {
                    keys.push(key);
                }
                last = i + 1;
            }
            _ => {}
        }
    }
    let part = content[last..].trim();
    if let Some(key) = extract_key(part) {
        keys.push(key);
    }
    keys
}

/// Extract the key name before `:` in an object entry.
/// Handles quoted keys: `'foo-bar'`, `"fooBar"`, and bare keys: `foo`.
fn extract_key(part: &str) -> Option<&str> {
    let part = part.trim();
    if part.is_empty() {
        return None;
    }
    // Quoted key
    if part.starts_with('"') || part.starts_with('\'') {
        let quote = part.chars().next()?;
        let key = &part[1..];
        let end = key.find(quote)?;
        return Some(key[..end].trim());
    }
    // Bare key: before `:`
    let end = part.find(':').unwrap_or(part.len());
    let key = part[..end].trim();
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
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
    use crate::analysis::ts_util::AttrData;

    #[test]
    fn test_is_valid_signal_name() {
        assert!(is_valid_signal_name("counter"));
        assert!(is_valid_signal_name("user-name"));
        assert!(is_valid_signal_name("my_signal"));
        assert!(!is_valid_signal_name(""));
        assert!(!is_valid_signal_name("my name"));
    }

    #[test]
    fn test_signal_names_from_attr_key_based() {
        let attr = AttrData {
            raw_name: "data-bind:foo".into(),
            plugin_name: "bind".into(),
            key: Some("foo".into()),
            name_start: 0, name_len: 0,
            value: None, value_start: None,
            modifiers: vec![],
            has_trailing_colon: false,
        };
        assert_eq!(signal_names_from_attr(&attr), vec!["foo"]);
    }

    #[test]
    fn test_signal_names_from_attr_value_based() {
        let attr = AttrData {
            raw_name: "data-bind".into(),
            plugin_name: "bind".into(),
            key: None,
            name_start: 0, name_len: 0,
            value: Some("percentage".into()), value_start: None,
            modifiers: vec![],
            has_trailing_colon: false,
        };
        assert_eq!(signal_names_from_attr(&attr), vec!["percentage"]);
    }

    #[test]
    fn test_signal_names_from_obj_literal() {
        let attr = AttrData {
            raw_name: "data-signals".into(),
            plugin_name: "signals".into(),
            key: None,
            name_start: 0, name_len: 0,
            value: Some("{percentage: 0, contents: 'hello', foo: 'bar'}".into()),
            value_start: None,
            modifiers: vec![],
            has_trailing_colon: false,
        };
        let mut names = signal_names_from_attr(&attr);
        names.sort();
        assert_eq!(names, vec!["contents", "foo", "percentage"]);
    }

    #[test]
    fn test_signal_names_from_quoted_obj_literal() {
        // Values are already unquoted by the extractor,
        // so this represents `data-signals="{percentage: 0, contents: 'hello'}"`
        let attr = AttrData {
            raw_name: "data-signals".into(),
            plugin_name: "signals".into(),
            key: None,
            name_start: 0, name_len: 0,
            value: Some("{percentage: 0, contents: 'hello'}".into()),
            value_start: None,
            modifiers: vec![],
            has_trailing_colon: false,
        };
        let mut names = signal_names_from_attr(&attr);
        names.sort();
        assert_eq!(names, vec!["contents", "percentage"]);
    }
}
