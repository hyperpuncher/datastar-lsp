use std::collections::{BTreeMap, BTreeSet};

/// Where a signal is defined in the DOM
#[derive(Debug, Clone)]
pub struct SignalDef {
    pub name: String,
    /// The attribute that defines this signal (e.g. "signals", "bind", "computed")
    pub defined_by: String,
    /// Byte offset in source
    pub byte_offset: usize,
}

/// Where a signal is referenced
#[derive(Debug, Clone)]
pub struct SignalRef {
    pub name: String,
    pub byte_offset: usize,
}

/// Result of analyzing a document's signals
#[derive(Debug, Default)]
pub struct SignalAnalysis {
    /// All defined signals (from data-signals, data-bind, data-computed, data-ref, data-indicator)
    pub definitions: BTreeMap<String, Vec<SignalDef>>,
    /// All signal references (from expression values: $foo, $foo.bar)
    pub references: Vec<SignalRef>,
    /// Top-level signal names defined in the document
    pub top_level_names: BTreeSet<String>,
}

/// Analyzes a document's text for signal definitions and references.
pub fn analyze_signals(text: &str) -> SignalAnalysis {
    let mut analysis = SignalAnalysis::default();

    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(pos) = bytes[i..].windows(5).position(|w| w == b"data-") {
            let start = i + pos;
            let after_data = &bytes[start + 5..];
            let colon_in_attr = after_data.iter().position(|&b| b == b':');
            let eq_or_end = after_data
                .iter()
                .position(|&b| b == b'=' || b == b' ' || b == b'>' || b == b'\n' || b == b'\r')
                .unwrap_or(after_data.len());

            let plugin = if let Some(colon) = colon_in_attr {
                if colon < eq_or_end {
                    std::str::from_utf8(&after_data[..colon]).unwrap_or("")
                } else {
                    std::str::from_utf8(&after_data[..eq_or_end]).unwrap_or("")
                }
            } else {
                std::str::from_utf8(&after_data[..eq_or_end]).unwrap_or("")
            };

            let is_signal_definer = matches!(
                plugin,
                "signals" | "bind" | "computed" | "ref" | "indicator" | "match-media"
            );

            if is_signal_definer {
                if let Some(rel_colon) = colon_in_attr.filter(|&c| c < eq_or_end) {
                    let key_start = start + 5 + rel_colon + 1;
                    let key_end = bytes[key_start..]
                        .iter()
                        .position(|&b| b == b'_' || b == b'=' || b == b' ' || b == b'>')
                        .map(|p| key_start + p)
                        .unwrap_or(bytes.len());
                    if key_start < key_end {
                        let signal_name = std::str::from_utf8(&bytes[key_start..key_end])
                            .unwrap_or("")
                            .to_string();
                        if !signal_name.is_empty()
                            && signal_name
                                .chars()
                                .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
                        {
                            analysis
                                .definitions
                                .entry(signal_name.clone())
                                .or_default()
                                .push(SignalDef {
                                    name: signal_name.clone(),
                                    defined_by: plugin.to_string(),
                                    byte_offset: start,
                                });
                            let top = signal_name.split('.').next().unwrap_or("").to_string();
                            if !top.is_empty() {
                                analysis.top_level_names.insert(top);
                            }
                        }
                    }
                }
            }

            i = start + 5;
        } else {
            break;
        }
    }

    // Scan for signal references: $foo, $foo.bar, $foo.bar.baz
    i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next.is_ascii_alphabetic() || next == b'_' {
                let mut j = i + 1;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric()
                        || bytes[j] == b'-'
                        || bytes[j] == b'_'
                        || bytes[j] == b'.'
                        || bytes[j] == b'['
                        || bytes[j] == b']')
                {
                    j += 1;
                }
                let raw_name = std::str::from_utf8(&bytes[i + 1..j]).unwrap_or("");
                let signal_name = raw_name
                    .trim_end_matches("++")
                    .trim_end_matches("--")
                    .trim_end_matches('+')
                    .trim_end_matches('-');
                if !signal_name.is_empty() {
                    analysis.references.push(SignalRef {
                        name: signal_name.to_string(),
                        byte_offset: i,
                    });
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }

    analysis
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_definitions() {
        let text =
            r#"<div data-signals:foo="1" data-bind:bar data-computed:baz="$foo + $bar"></div>"#;
        let analysis = analyze_signals(text);
        assert!(analysis.definitions.contains_key("foo"));
        assert!(analysis.definitions.contains_key("bar"));
        assert!(analysis.definitions.contains_key("baz"));
        assert!(analysis.top_level_names.contains("foo"));
        assert!(analysis.top_level_names.contains("bar"));
        assert!(analysis.top_level_names.contains("baz"));
    }

    #[test]
    fn test_signal_references() {
        let analysis = analyze_signals(
            r#"<div data-text="$user.name" data-show="$count > 5" data-class:active="$items[0]"></div>"#,
        );
        assert!(analysis.references.iter().any(|r| r.name == "user.name"));
        assert!(analysis.references.iter().any(|r| r.name == "count"));
        assert!(
            analysis.references.iter().any(|r| r.name.contains("items")),
            "expected items in refs: {:?}",
            analysis
                .references
                .iter()
                .map(|r| &r.name)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_undefined_signals() {
        let analysis =
            analyze_signals(r#"<div data-text="$undefined_signal" data-signals:foo="1"></div>"#);
        let undefined: Vec<_> = analysis
            .references
            .iter()
            .filter(|r| {
                let top = r.name.split('.').next().unwrap_or("");
                !analysis.top_level_names.contains(top)
            })
            .collect();
        assert!(!undefined.is_empty());
    }

    #[test]
    fn test_bind_defines_signal() {
        let text = r#"<input data-bind:count /><button data-on:click="$count++">+</button>"#;
        let a = analyze_signals(text);
        assert!(
            a.top_level_names.contains("count"),
            "data-bind:count should define count, got: {:?}",
            a.top_level_names
        );
    }
}
