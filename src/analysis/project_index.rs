use std::sync::Arc;

use dashmap::DashMap;
use tower_lsp::lsp_types::Url;

use crate::analysis::signals::SignalAnalysis;
use crate::line_index::LineIndex;
use crate::parser::html::DataAttribute;

/// Full document state: line index, parsed attributes, signal analysis.
type DocumentEntry = (LineIndex, Vec<DataAttribute>, SignalAnalysis);

/// Project-wide index across all open documents.
/// Caches parsed state so no handler needs to re-parse text.
pub struct ProjectIndex {
    /// Document state per file: (raw text, parsed attrs, signal analysis)
    documents: Arc<DashMap<Url, DocumentEntry>>,
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

    /// Index a document: store line index, parsed attributes, and signal analysis.
    pub fn index(
        &self,
        uri: &Url,
        text: String,
        attrs: Vec<DataAttribute>,
        analysis: SignalAnalysis,
    ) {
        let line_index = LineIndex::new(text);
        self.documents
            .insert(uri.clone(), (line_index, attrs, analysis));
    }

    /// Remove a document from the index.
    pub fn remove(&self, uri: &Url) {
        self.documents.remove(uri);
    }

    /// Get the full document entry for a URI.
    pub fn get(&self, uri: &Url) -> Option<DocumentEntry> {
        self.documents.get(uri).map(|r| r.clone())
    }

    /// Get the raw text for a URI.
    pub fn text(&self, uri: &Url) -> Option<String> {
        self.documents.get(uri).map(|r| r.0.text().to_string())
    }

    /// Get the line index for a URI.
    pub fn line_index(&self, uri: &Url) -> Option<LineIndex> {
        self.documents.get(uri).map(|r| r.0.clone())
    }

    /// Get parsed attributes for a URI.
    pub fn attrs(&self, uri: &Url) -> Option<Vec<DataAttribute>> {
        self.documents.get(uri).map(|r| r.1.clone())
    }

    /// Get signal analysis for a URI.
    pub fn analysis(&self, uri: &Url) -> Option<SignalAnalysis> {
        self.documents.get(uri).map(|r| r.2.clone())
    }

    /// Iterate over all documents.
    pub fn iter(&self) -> dashmap::iter::Iter<'_, Url, DocumentEntry> {
        self.documents.iter()
    }

    /// Find the definition of a top-level signal across all indexed documents.
    /// Returns list of (url, byte_offset) for byte-to-position.
    pub fn find_definitions(&self, top_name: &str, exclude_uri: Option<&Url>) -> Vec<(Url, usize)> {
        let mut results = Vec::new();
        for entry in self.documents.iter() {
            let uri = entry.key();
            if let Some(exclude) = exclude_uri {
                if uri == exclude {
                    continue;
                }
            }
            if let Some(defs) = entry.value().2.definitions.get(top_name) {
                for def in defs {
                    results.push((uri.clone(), def.byte_offset));
                }
            }
        }
        results
    }

    /// Find all references to a signal across all indexed documents.
    /// Returns list of (url, byte_offset, ref_name_len).
    pub fn find_all_references(&self, top_name: &str) -> Vec<(Url, usize, usize)> {
        let mut results = Vec::new();
        for entry in self.documents.iter() {
            let uri = entry.key().clone();
            for ref_ in &entry.value().2.references {
                let ref_top = ref_.name.split('.').next().unwrap_or("");
                if ref_top == top_name {
                    results.push((uri.clone(), ref_.byte_offset, ref_.name.len() + 1));
                }
            }
            if let Some(defs) = entry.value().2.definitions.get(top_name) {
                for def in defs {
                    results.push((uri.clone(), def.byte_offset, def.name.len() + 1));
                }
            }
        }
        results
    }
}
