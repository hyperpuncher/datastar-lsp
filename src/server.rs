use std::sync::Arc;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use datastar_lsp::analysis::{
    code_actions, completions, diagnostics, goto_def, hover, project_index::ProjectIndex,
    references, rename,
};

pub struct Backend {
    client: Client,
    project_index: Arc<ProjectIndex>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            project_index: Arc::new(ProjectIndex::new()),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "$".to_string(),
                        "@".to_string(),
                        ":".to_string(),
                        "_".to_string(),
                        ".".to_string(),
                    ]),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(false),
                    work_done_progress_options: Default::default(),
                })),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "datastar-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "datastar-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();
        tracing::info!("Opened: {uri}");
        self.project_index.index(&uri, text.clone());
        self.publish_diagnostics(&uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.content_changes[0].text.clone();
        self.project_index.index(&uri, text.clone());
        self.publish_diagnostics(&uri, &text).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        tracing::info!("Closed: {uri}");
        self.project_index.remove(&uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let (line_index, text) = match self.project_index.get(uri) {
            Some(e) => e,
            None => return Ok(None),
        };

        Ok(hover::generate(&line_index, &text, position, uri))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let (line_index, text) = match self.project_index.get(uri) {
            Some(e) => e,
            None => return Ok(None),
        };

        let items = completions::generate(&line_index, &text, position, uri);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let (line_index, text) = match self.project_index.get(uri) {
            Some(e) => e,
            None => return Ok(None),
        };

        Ok(goto_def::goto_definition(
            &line_index,
            &text,
            position,
            uri,
            Some(&self.project_index),
        ))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let (line_index, text) = match self.project_index.get(uri) {
            Some(e) => e,
            None => return Ok(None),
        };

        Ok(Some(references::find_references(
            &line_index,
            &text,
            position,
            uri,
            Some(&self.project_index),
        )))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;

        let (line_index, _text) = match self.project_index.get(uri) {
            Some(e) => e,
            None => return Ok(None),
        };

        let mut actions = Vec::new();
        for diag in &params.context.diagnostics {
            if diag.source.as_deref() == Some("datastar") {
                actions.extend(code_actions::generate(&line_index, uri, diag));
            }
        }
        Ok(Some(actions))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = &params.new_name;

        let (line_index, text) = match self.project_index.get(uri) {
            Some(e) => e,
            None => return Ok(None),
        };

        let result = rename::rename_signal(
            &line_index,
            &text,
            position,
            uri,
            new_name,
            Some(&self.project_index),
        );
        match result {
            Some(changes) => Ok(Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            })),
            None => Ok(None),
        }
    }
}

impl Backend {
    async fn publish_diagnostics(&self, uri: &Url, text: &str) {
        let (line_index, _) = match self.project_index.get(uri) {
            Some(e) => e,
            None => return,
        };
        let diags = diagnostics::generate(&line_index, text, uri, Some(&self.project_index));
        self.client
            .publish_diagnostics(uri.clone(), diags, None)
            .await;
    }
}
