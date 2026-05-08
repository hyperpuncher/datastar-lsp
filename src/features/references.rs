use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Position, ReferenceParams};

use super::super::analysis;
use crate::server::Backend;

/// Handle references request.
pub async fn find_references(
	backend: &Backend,
	params: ReferenceParams,
) -> Result<Option<Vec<Location>>> {
	let uri = &params.text_document_position.text_document.uri;
	let position = params.text_document_position.position;

	let text = backend.documents.get(uri).map(|t| t.clone());

	match text {
		Some(text) => {
			let locs = analysis::references::find_references(&text, position, uri);
			Ok(Some(locs))
		}
		None => Ok(None),
	}
}
