use std::sync::Arc;

use dashmap::DashMap;
use tower_lsp::lsp_types::Url;

use crate::analysis::signals::{SignalAnalysis, SignalDef};

/// Project-wide signal index across all open documents.
/// Used for cross-file go-to-definition, references, and rename.
pub struct ProjectIndex {
    /// Document state per file: (raw text, signal analysis)
    pub documents: Arc<DashMap<Url, (String, SignalAnalysis)>>,
}

impl ProjectIndex {
    pub fn new() -> Self {
        Self {
            documents: Arc::new(DashMap::new()),
        }
    }

    /// Index a document's text and signal analysis.
    pub fn index(&self, uri: &Url, text: String, analysis: SignalAnalysis) {
        self.documents.insert(uri.clone(), (text, analysis));
    }

    /// Remove a document from the index.
    pub fn remove(&self, uri: &Url) {
        self.documents.remove(uri);
    }

    /// Find the definition of a top-level signal across all indexed documents.
    /// Returns list of (url, text_ref, SignalDef) for byte-to-position.
    pub fn find_definitions(&self, top_name: &str, exclude_uri: Option<&Url>) -> Vec<(Url, usize)> {
        let mut results = Vec::new();
        for entry in self.documents.iter() {
            let uri = entry.key();
            if let Some(exclude) = exclude_uri {
                if uri == exclude {
                    continue;
                }
            }
            if let Some(defs) = entry.value().1.definitions.get(top_name) {
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
            for ref_ in &entry.value().1.references {
                let ref_top = ref_.name.split('.').next().unwrap_or("");
                if ref_top == top_name {
                    results.push((uri.clone(), ref_.byte_offset, ref_.name.len() + 1));
                }
            }
            if let Some(defs) = entry.value().1.definitions.get(top_name) {
                for def in defs {
                    results.push((uri.clone(), def.byte_offset, def.name.len() + 1));
                }
            }
        }
        results
    }
}
