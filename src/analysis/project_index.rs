use std::sync::Arc;

use dashmap::DashMap;
use tower_lsp::lsp_types::Url;

use crate::line_index::LineIndex;

/// Project-wide index across all open documents.
/// Stores raw text and line index per file.
/// All tree-sitter parsing happens on-demand in handlers.
pub struct ProjectIndex {
    documents: Arc<DashMap<Url, (LineIndex, String)>>,
}

impl Default for ProjectIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectIndex {
    pub fn new() -> Self {
        Self {
            documents: Arc::new(DashMap::new()),
        }
    }

    pub fn index(&self, uri: &Url, text: String) {
        let line_index = LineIndex::new(text.clone());
        self.documents.insert(uri.clone(), (line_index, text));
    }

    pub fn remove(&self, uri: &Url) {
        self.documents.remove(uri);
    }

    pub fn get(&self, uri: &Url) -> Option<(LineIndex, String)> {
        self.documents.get(uri).map(|r| r.clone())
    }

    pub fn text(&self, uri: &Url) -> Option<String> {
        self.documents.get(uri).map(|r| r.1.clone())
    }

    pub fn line_index(&self, uri: &Url) -> Option<LineIndex> {
        self.documents.get(uri).map(|r| r.0.clone())
    }

    pub fn iter(&self) -> dashmap::iter::Iter<'_, Url, (LineIndex, String)> {
        self.documents.iter()
    }
}
