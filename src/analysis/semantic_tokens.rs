use tower_lsp::lsp_types::{Position, SemanticToken, SemanticTokenType};

use crate::parser::html::DataAttribute;

/// Known semantic token types for Datastar.
const SIGNAL_TOKEN: u32 = 0; // $signal references
const ACTION_TOKEN: u32 = 1; // @action calls
const PLUGIN_TOKEN: u32 = 2; // data-plugin names
const MODIFIER_TOKEN: u32 = 3; // __modifier keys
const KEY_TOKEN: u32 = 4; // :key values (event names, signal names)

/// Generate semantic tokens for a document.
pub fn generate(text: &str, attrs: &[DataAttribute]) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();

    // Highlight attribute names
    for attr in attrs {
        // The data- prefix position
        let data_pos = byte_to_position(text, attr.name_start);
        // Plugin name starts at "data-" offset + 5
        let _plugin_start = attr.name_start + 5;
        let plugin_len = attr.plugin_name.len() as u32;
        tokens.push(SemanticToken {
            delta_line: data_pos.line,
            delta_start: data_pos.character + 5, // after "data-"
            length: plugin_len,
            token_type: PLUGIN_TOKEN,
            token_modifiers_bitset: 0,
        });

        // Key (after :)
        if let Some(key) = &attr.key {
            let colon = attr.raw_name.find(':').unwrap_or(0);
            let key_start = attr.name_start + colon + 1;
            let key_pos = byte_to_position(text, key_start);
            tokens.push(SemanticToken {
                delta_line: key_pos.line,
                delta_start: key_pos.character,
                length: key.len() as u32,
                token_type: KEY_TOKEN,
                token_modifiers_bitset: 0,
            });
        }

        // Modifiers (__mod_key.tags)
        let mut after_plugin = &attr.raw_name[5 + plugin_len as usize..];
        if attr.key.is_some() {
            if let Some(colon) = after_plugin.find(':') {
                after_plugin = &after_plugin[colon + 1..];
                if let Some(key_end) = after_plugin.find("__") {
                    after_plugin = &after_plugin[key_end..];
                } else {
                    after_plugin = "";
                }
            }
        }
        // Parse __mod.key.tags from after_plugin
        for part in after_plugin.split("__").filter(|s| !s.is_empty()) {
            let mod_pos_in = after_plugin.find(part).unwrap_or(0);
            if let Some(dot) = part.find('.') {
                tokens.push(SemanticToken {
                    delta_line: 0,
                    delta_start: (attr.name_start
                        + 5
                        + plugin_len as usize
                        + after_plugin.as_ptr() as usize
                        - attr.raw_name.as_ptr() as usize
                        + mod_pos_in) as u32,
                    length: dot as u32,
                    token_type: MODIFIER_TOKEN,
                    token_modifiers_bitset: 0,
                });
            } else {
                // Position calculation is approximate for modifiers — use the raw_name
                let raw_mod_pos = attr.raw_name.find(&format!("__{}", part));
                if let Some(pos) = raw_mod_pos {
                    let mod_pos = byte_to_position(text, attr.name_start + pos + 2);
                    tokens.push(SemanticToken {
                        delta_line: mod_pos.line,
                        delta_start: mod_pos.character,
                        length: part.len() as u32,
                        token_type: MODIFIER_TOKEN,
                        token_modifiers_bitset: 0,
                    });
                }
            }
        }

        // Highlight signal references ($foo) and action calls (@action) in values
        if let (Some(value_start), Some(value)) = (attr.value_start, &attr.value) {
            let bytes = value.as_bytes();
            let mut i = 0;
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
                        // Position is value_start + 1 (past quote) + i
                        // Trim trailing ++/-- from signal name for token length
                        let raw_name = std::str::from_utf8(&bytes[i..j]).unwrap_or("");
                        let clean_name = raw_name
                            .trim_end_matches("++")
                            .trim_end_matches("--")
                            .trim_end_matches('+')
                            .trim_end_matches('-');
                        let clean_len = clean_name.len();
                        if clean_len == 0 {
                            i = j;
                            continue;
                        }
                        let sig_start = value_start + 1 + i;
                        let sig_pos = byte_to_position(text, sig_start);
                        tokens.push(SemanticToken {
                            delta_line: sig_pos.line,
                            delta_start: sig_pos.character,
                            length: clean_len as u32,
                            token_type: SIGNAL_TOKEN,
                            token_modifiers_bitset: 0,
                        });
                        i = j;
                        continue;
                    }
                }
                if bytes[i] == b'@' && i + 1 < bytes.len() {
                    let next = bytes[i + 1];
                    if next.is_ascii_alphabetic() || next == b'_' {
                        let mut j = i + 1;
                        while j < bytes.len()
                            && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
                        {
                            j += 1;
                        }
                        let act_start = value_start + 1 + i;
                        let act_pos = byte_to_position(text, act_start);
                        tokens.push(SemanticToken {
                            delta_line: act_pos.line,
                            delta_start: act_pos.character,
                            length: (j - i) as u32,
                            token_type: ACTION_TOKEN,
                            token_modifiers_bitset: 0,
                        });
                        i = j;
                        continue;
                    }
                }
                i += 1;
            }
        }
    }

    // Sort by line, then character (required for delta encoding)
    tokens.sort_by_key(|t| (t.delta_line, t.delta_start));

    // Convert absolute positions to delta positions
    let mut last_line = 0u32;
    let mut last_start = 0u32;
    for token in &mut *tokens {
        let current_line = token.delta_line;
        let current_start = token.delta_start;

        if current_line == last_line {
            token.delta_start = current_start - last_start;
        } else {
            token.delta_line = current_line - last_line;
            token.delta_start = current_start;
        }

        last_line = current_line;
        last_start = current_start;
    }

    tokens
}

/// Returns the semantic token legend (token types and modifiers).
pub fn legend() -> tower_lsp::lsp_types::SemanticTokensLegend {
    tower_lsp::lsp_types::SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::VARIABLE, // SIGNAL_TOKEN = 0
            SemanticTokenType::FUNCTION, // ACTION_TOKEN = 1
            SemanticTokenType::KEYWORD,  // PLUGIN_TOKEN = 2
            SemanticTokenType::MODIFIER, // MODIFIER_TOKEN = 3
            SemanticTokenType::PROPERTY, // KEY_TOKEN = 4
        ],
        token_modifiers: vec![],
    }
}

fn byte_to_position(text: &str, byte_offset: usize) -> Position {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_tokens_generated() {
        let html = r#"<div data-signals:foo="1" data-on:click__debounce.500ms="@get('/api')"><span data-text="$foo"></span></div>"#;
        let parsed = crate::parser::html::parse_html(html.as_bytes()).unwrap();
        let tokens = generate(html, &parsed.1);
        assert!(!tokens.is_empty(), "should generate semantic tokens");

        // Check we have signal tokens
        let has_signal = tokens.iter().any(|t| t.token_type == SIGNAL_TOKEN);
        assert!(has_signal, "should have signal token for $foo");

        // Check we have action tokens
        let has_action = tokens.iter().any(|t| t.token_type == ACTION_TOKEN);
        assert!(has_action, "should have action token for @get");

        // Check we have plugin tokens
        let has_plugin = tokens.iter().any(|t| t.token_type == PLUGIN_TOKEN);
        assert!(has_plugin, "should have plugin token for data-*");

        // Check we have modifier tokens
        let has_modifier = tokens.iter().any(|t| t.token_type == MODIFIER_TOKEN);
        assert!(has_modifier, "should have modifier token for __debounce");

        // Delta encoding: verify tokens are in order
        let mut last_line = 0;
        let mut last_char = 0;
        for token in &tokens {
            let line =
                token
                    .delta_line
                    .wrapping_add(if token.delta_line == 0 { 0 } else { last_line });
            let char_pos = if token.delta_line == 0 {
                token.delta_start.wrapping_add(last_char)
            } else {
                token.delta_start
            };
            assert!(line >= last_line);
            if line == last_line {
                assert!(char_pos >= last_char);
            }
            last_line = line;
            last_char = char_pos;
        }
    }
}
