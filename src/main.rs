use tower_lsp::{LspService, Server};

mod server;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let (service, socket) = LspService::new(server::Backend::new);
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
