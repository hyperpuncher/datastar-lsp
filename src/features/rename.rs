use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{Position, RenameParams, WorkspaceEdit};

use super::super::analysis;
use crate::server::Backend;

/// Handle rename request.
/// Renames a signal across the entire document.
pub async fn rename(
	backend: &Backend,
	params: RenameParams,
) -> Result<Option<WorkspaceEdit>> {
	let uri = &params.text_document_position.text_document.uri;
	let position = params.text_document_position.position;
	let new_name = &params.new_name;

	let text = backend.documents.get(uri).map(|t| t.clone());

	match text {
		Some(text) => {
			// The WorkspaceEdit from rename_signal uses a dummy URI.
			// Replace it with the actual URI.
			let mut edit = analysis::rename::rename_signal(&text, position, new_name)
				.unwrap_or_default();
			if let Some(ref mut changes) = edit.changes {
				// Move all changes to the actual URI
				let entries: Vec<_> = changes
					.iter()
					.flat_map(|(_, edits)| edits.clone())
					.collect();
				changes.clear();
				changes.insert(uri.clone(), entries);
			}
			Ok(Some(edit))
		}
		None => Ok(None),
	}
}
