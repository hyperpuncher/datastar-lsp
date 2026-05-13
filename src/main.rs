use tower_lsp::{LspService, Server};

mod server;

#[tokio::main]
async fn main() {
    // Handle --version before starting LSP
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("datastar-lsp {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let (service, socket) = LspService::new(server::Backend::new);
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
