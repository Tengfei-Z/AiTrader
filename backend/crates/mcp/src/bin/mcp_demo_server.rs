use anyhow::Result;
use mcp_adapter::DemoArithmeticServer;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    DemoArithmeticServer::new().serve_stdio().await
}

fn init_tracing() {
    let subscriber = fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}
