use std::sync::Arc;

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use datastar_lsp::analysis::{
    code_actions, completions, diagnostics, goto_def, hover, project_index::ProjectIndex,
    references, rename,
};
use datastar_lsp::parser::html::{self, DataAttribute};

pub struct Backend {
    client: Client,
    documents: Arc<DashMap<Url, String>>,
    project_index: Arc<ProjectIndex>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(DashMap::new()),
            project_index: Arc::new(ProjectIndex::new()),
        }
    }

    /// Parse document using the appropriate tree-sitter language.
    fn parse_document(&self, uri: &Url, text: &str) -> Vec<DataAttribute> {
        let path = uri.path();
        if path.ends_with(".jsx") || path.ends_with(".tsx") {
            html::parse_jsx(text.as_bytes())
                .map(|(_, a)| a)
                .unwrap_or_default()
        } else {
            html::parse_html(text.as_bytes())
                .map(|(_, a)| a)
                .unwrap_or_default()
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

        // Index signals for cross-file tracking
        let analysis = datastar_lsp::analysis::signals::analyze_signals(&text);
        self.project_index.index(&uri, text.clone(), analysis);

        self.documents.insert(uri.clone(), text.clone());
        self.publish_diagnostics(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.content_changes[0].text.clone();

        // Re-index signals
        let analysis = datastar_lsp::analysis::signals::analyze_signals(&text);
        self.project_index.index(&uri, text.clone(), analysis);

        self.documents.insert(uri.clone(), text.clone());
        self.publish_diagnostics(uri, &text).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        tracing::info!("Closed: {uri}");
        self.documents.remove(&uri);
        self.project_index.remove(&uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let text = match self.documents.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let attrs = self.parse_document(uri, &text);
        Ok(hover::generate(&text, position, &attrs))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let text = match self.documents.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let data_attrs = self.parse_document(uri, &text);

        let ctx = completions::CompletionContext {
            text: text.clone(),
            position,
            data_attrs,
        };

        let items = completions::generate(&ctx);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let text = match self.documents.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let attrs = self.parse_document(uri, &text);

        Ok(goto_def::goto_definition(
            &text,
            position,
            uri,
            &attrs,
            Some(&self.project_index),
        ))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let text = match self.documents.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        Ok(Some(references::find_references(
            &text,
            position,
            uri,
            Some(&self.project_index),
        )))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let text = match self.documents.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let mut actions = Vec::new();
        for diag in &params.context.diagnostics {
            if diag.source.as_deref() == Some("datastar") {
                actions.extend(code_actions::generate(&text, uri, diag));
            }
        }

        Ok(Some(actions))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = &params.new_name;

        let text = match self.documents.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        match rename::rename_signal(&text, position, uri, new_name, Some(&self.project_index)) {
            Some(changes) => Ok(Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            })),
            None => Ok(None),
        }
    }
}

impl Backend {
    async fn publish_diagnostics(&self, uri: Url, text: &str) {
        let diags = diagnostics::generate(text, Some(&self.project_index));
        self.client.publish_diagnostics(uri, diags, None).await;
    }
}
