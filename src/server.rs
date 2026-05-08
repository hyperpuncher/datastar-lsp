use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

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
    project_index: Arc<ProjectIndex>,
    /// Monotonic document version — incremented on each did_change.
    /// Handlers check this before returning results.
    doc_version: AtomicU64,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            project_index: Arc::new(ProjectIndex::new()),
            doc_version: AtomicU64::new(0),
        }
    }

    /// Parse document and index all state (text, attrs, signal analysis).
    fn index_document(&self, uri: &Url, text: &str) {
        let attrs = self.parse_document(uri, text);
        let analysis = datastar_lsp::analysis::signals::analyze_signals(text);
        self.project_index
            .index(uri, text.to_string(), attrs, analysis);
    }

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

        self.index_document(&uri, &text);
        self.publish_diagnostics(&uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.content_changes[0].text.clone();

        self.doc_version.fetch_add(1, Ordering::Release);
        self.index_document(&uri, &text);
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
        let version = self.doc_version.load(Ordering::Acquire);

        let line_index = match self.project_index.line_index(uri) {
            Some(li) => li,
            None => return Ok(None),
        };
        let analysis = match self.project_index.analysis(uri) {
            Some(a) => a,
            None => return Ok(None),
        };

        let result = hover::generate(
            &line_index,
            position,
            &self.project_index.attrs(uri).unwrap_or_default(),
            &analysis,
        );

        if self.doc_version.load(Ordering::Acquire) != version {
            return Ok(None);
        }
        Ok(result)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let version = self.doc_version.load(Ordering::Acquire);

        let line_index = match self.project_index.line_index(uri) {
            Some(li) => li,
            None => return Ok(None),
        };

        let data_attrs = self.project_index.attrs(uri).unwrap_or_default();

        let ctx = completions::CompletionContext {
            line_index: &line_index,
            position,
            data_attrs,
        };

        let items = completions::generate(&ctx);
        if self.is_stale(version) {
            return Ok(None);
        }
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let version = self.doc_version.load(Ordering::Acquire);

        let line_index = match self.project_index.line_index(uri) {
            Some(li) => li,
            None => return Ok(None),
        };
        let attrs = self.project_index.attrs(uri).unwrap_or_default();
        let analysis = match self.project_index.analysis(uri) {
            Some(a) => a,
            None => return Ok(None),
        };

        let result = goto_def::goto_definition(
            &line_index,
            position,
            uri,
            &attrs,
            &analysis,
            Some(&self.project_index),
        );
        if self.is_stale(version) {
            return Ok(None);
        }
        Ok(result)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let version = self.doc_version.load(Ordering::Acquire);

        let line_index = match self.project_index.line_index(uri) {
            Some(li) => li,
            None => return Ok(None),
        };
        let analysis = match self.project_index.analysis(uri) {
            Some(a) => a,
            None => return Ok(None),
        };

        let result = Some(references::find_references(
            &line_index,
            position,
            uri,
            &analysis,
            Some(&self.project_index),
        ));
        if self.is_stale(version) {
            return Ok(None);
        }
        Ok(result)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let line_index = match self.project_index.line_index(uri) {
            Some(li) => li,
            None => return Ok(None),
        };

        let mut actions = Vec::new();
        for diag in &params.context.diagnostics {
            if diag.source.as_deref() == Some("datastar") {
                actions.extend(code_actions::generate(line_index.text(), uri, diag));
            }
        }

        Ok(Some(actions))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = &params.new_name;
        let version = self.doc_version.load(Ordering::Acquire);

        let line_index = match self.project_index.line_index(uri) {
            Some(li) => li,
            None => return Ok(None),
        };
        let analysis = match self.project_index.analysis(uri) {
            Some(a) => a,
            None => return Ok(None),
        };

        let result = rename::rename_signal(
            &line_index,
            position,
            uri,
            new_name,
            &analysis,
            Some(&self.project_index),
        );
        if self.is_stale(version) {
            return Ok(None);
        }
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
    /// Check if document version changed since we started processing.
    fn is_stale(&self, version: u64) -> bool {
        self.doc_version.load(Ordering::Acquire) != version
    }

    async fn publish_diagnostics(&self, uri: &Url, text: &str) {
        let diags = diagnostics::generate(text, Some(&self.project_index));
        self.client
            .publish_diagnostics(uri.clone(), diags, None)
            .await;
    }
}
