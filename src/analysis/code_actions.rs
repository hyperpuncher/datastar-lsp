use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Diagnostic, Range, TextEdit, WorkspaceEdit,
};

/// Generate code actions for a given diagnostic.
pub fn generate(
    text: &str,
    uri: &tower_lsp::lsp_types::Url,
    diagnostic: &Diagnostic,
) -> Vec<CodeActionOrCommand> {
    let msg = &diagnostic.message;
    let range = diagnostic.range;

    if msg.starts_with("Signal not defined locally") {
        return generate_define_signal(text, uri, range, msg);
    }

    if msg.starts_with("Missing value:") {
        return generate_add_value(uri, range);
    }

    if msg.starts_with("Missing key:") {
        return generate_add_key(uri, range);
    }

    if msg.starts_with("Unknown Datastar attribute") {
        return generate_suggest_attr(uri, range);
    }

    vec![]
}

/// Code action: Add a data-signals definition for an undefined signal.
fn generate_define_signal(
    text: &str,
    uri: &tower_lsp::lsp_types::Url,
    _range: Range,
    msg: &str,
) -> Vec<CodeActionOrCommand> {
    // Extract signal name from message: "Signal not defined locally: '$foo'..."
    let signal_name = msg.split('\'').nth(1).unwrap_or("").trim_start_matches('$');

    if signal_name.is_empty() {
        return vec![];
    }

    // Find the nearest insertion point — after the closest parent element's opening tag,
    // or at the beginning of the document
    let insertion_offset = find_signal_insertion_point(text);

    let insert_pos = crate::util::byte_to_position(text, insertion_offset);
    let edit_text = format!("\n\t<div data-signals:{}=\"\" hidden></div>", signal_name);

    let edit = TextEdit {
        range: Range {
            start: insert_pos,
            end: insert_pos,
        },
        new_text: edit_text,
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    vec![CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Define signal '${}' at top of document", signal_name),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic::clone_diagnostic(&Diagnostic {
            message: msg.to_string(),
            ..Default::default()
        })]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }),
        ..Default::default()
    })]
}

/// Find the best insertion point for a new data-signals element.
/// Inserts right after the first opening tag (after `>`) of a container div/body.
fn find_signal_insertion_point(text: &str) -> usize {
    let bytes = text.as_bytes();

    // Try to find <body> or first <div>
    for pattern in &[
        b"<body>" as &[u8],
        b"<body " as &[u8],
        b"<div " as &[u8],
        b"<div>" as &[u8],
    ] {
        if let Some(pos) = bytes.windows(pattern.len()).position(|w| w == *pattern) {
            // Find the closing > of this tag
            let after = &bytes[pos..];
            if let Some(gt) = after.iter().position(|&b| b == b'>') {
                return pos + gt + 1;
            }
        }
    }

    // Fallback: start of document
    0
}

/// Code action: Add an empty value to an attribute.
fn generate_add_value(uri: &tower_lsp::lsp_types::Url, range: Range) -> Vec<CodeActionOrCommand> {
    let edit = TextEdit {
        range: Range {
            start: range.end,
            end: range.end,
        },
        new_text: "=\"\"".to_string(),
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    vec![CodeActionOrCommand::CodeAction(CodeAction {
        title: "Add empty value".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }),
        ..Default::default()
    })]
}

/// Code action: Add a key to an attribute.
fn generate_add_key(uri: &tower_lsp::lsp_types::Url, range: Range) -> Vec<CodeActionOrCommand> {
    let edit = TextEdit {
        range: Range {
            start: range.end,
            end: range.end,
        },
        new_text: ":".to_string(),
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    vec![CodeActionOrCommand::CodeAction(CodeAction {
        title: "Add key separator \":\"".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }),
        ..Default::default()
    })]
}

/// Code action: Suggest closest attribute name for unknown attribute.
fn generate_suggest_attr(
    _uri: &tower_lsp::lsp_types::Url,
    _range: Range,
) -> Vec<CodeActionOrCommand> {
    // Just provide a hint to check the docs
    vec![CodeActionOrCommand::CodeAction(CodeAction {
        title: "Check Datastar attribute reference".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: None,
        ..Default::default()
    })]
}

/// Helper to clone a Diagnostic with default fields filled in.
mod diagnostic {
    use tower_lsp::lsp_types::Diagnostic;
    pub fn clone_diagnostic(_d: &Diagnostic) -> Diagnostic {
        Diagnostic::default()
    }
}
