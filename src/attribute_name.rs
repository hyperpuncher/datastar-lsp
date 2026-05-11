/// Parsed components of a `data-*` attribute name.
pub struct ParsedKey {
    pub plugin: String,
    pub key: Option<String>,
    pub modifiers: Vec<(String, Vec<String>)>,
}

/// Parse a `data-*` attribute name into plugin, key, and modifiers.
/// e.g. `data-on:click__debounce.500ms` → plugin="on", key="click", modifiers=[("debounce", ["500ms"])]
///
/// Tree-sitter HTML/TSX grammars do not decompose `__modifier.tag` or `:key`
/// within attribute names, so we parse raw text here.
pub fn parse_attribute_key(raw: &str) -> ParsedKey {
    let rest = &raw[5..];

    let (base, modifier_str) = match rest.find("__") {
        Some(pos) => (&rest[..pos], Some(&rest[pos + 2..])),
        None => (rest, None),
    };

    let (plugin, key) = match base.find(':') {
        Some(pos) => (&base[..pos], Some(base[pos + 1..].to_string())),
        None => (base, None),
    };

    let modifiers = modifier_str
        .unwrap_or("")
        .split("__")
        .filter(|s| !s.is_empty())
        .filter_map(|mod_part| {
            let mut parts = mod_part.splitn(2, '.');
            let key = parts.next()?.to_string();
            let tags: Vec<String> = parts
                .flat_map(|s| s.split('.'))
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            Some((key, tags))
        })
        .collect();

    ParsedKey {
        plugin: plugin.to_string(),
        key,
        modifiers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_attribute_key_simple() {
        let p = parse_attribute_key("data-show");
        assert_eq!(p.plugin, "show");
        assert!(p.key.is_none());
        assert!(p.modifiers.is_empty());
    }

    #[test]
    fn test_parse_attribute_key_with_key() {
        let p = parse_attribute_key("data-on:click");
        assert_eq!(p.plugin, "on");
        assert_eq!(p.key.as_deref(), Some("click"));
        assert!(p.modifiers.is_empty());
    }

    #[test]
    fn test_parse_attribute_key_with_modifiers() {
        let p = parse_attribute_key("data-on:click__debounce.500ms.leading");
        assert_eq!(p.plugin, "on");
        assert_eq!(p.key.as_deref(), Some("click"));
        assert_eq!(p.modifiers.len(), 1);
        assert_eq!(p.modifiers[0].0, "debounce");
        assert_eq!(p.modifiers[0].1, vec!["500ms", "leading"]);
    }

    #[test]
    fn test_parse_attribute_key_multiple_modifiers() {
        let p = parse_attribute_key("data-on:click__window__debounce.500ms");
        assert_eq!(p.plugin, "on");
        assert_eq!(p.key.as_deref(), Some("click"));
        assert_eq!(p.modifiers.len(), 2);
        assert_eq!(p.modifiers[0].0, "window");
        assert_eq!(p.modifiers[1].0, "debounce");
    }

    #[test]
    fn test_parse_attribute_key_no_key_with_modifier() {
        let p = parse_attribute_key("data-on-raf__throttle.10ms");
        assert_eq!(p.plugin, "on-raf");
        assert!(p.key.is_none());
        assert_eq!(p.modifiers.len(), 1);
        assert_eq!(p.modifiers[0].0, "throttle");
        assert_eq!(p.modifiers[0].1, vec!["10ms"]);
    }
}
