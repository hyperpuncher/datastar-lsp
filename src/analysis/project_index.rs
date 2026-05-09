use std::sync::Arc;

use dashmap::DashMap;
use tower_lsp::lsp_types::Url;

use crate::line_index::LineIndex;

/// Project-wide index across all open documents.
/// Stores raw text and line index per file.
/// All tree-sitter parsing happens on-demand in handlers.
pub struct ProjectIndex {
    documents: Arc<DashMap<Url, LineIndex>>,
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
        let line_index = LineIndex::new(text);
        self.documents.insert(uri.clone(), line_index);
    }

    pub fn remove(&self, uri: &Url) {
        self.documents.remove(uri);
    }

    pub fn get(&self, uri: &Url) -> Option<(LineIndex, String)> {
        self.documents.get(uri).map(|r| {
            let li = r.clone();
            let text = li.text().to_string();
            (li, text)
        })
    }

    pub fn text(&self, uri: &Url) -> Option<String> {
        self.documents.get(uri).map(|r| r.text().to_string())
    }

    pub fn line_index(&self, uri: &Url) -> Option<LineIndex> {
        self.documents.get(uri).map(|r| r.clone())
    }

    pub fn iter(&self) -> dashmap::iter::Iter<'_, Url, LineIndex> {
        self.documents.iter()
    }
}
