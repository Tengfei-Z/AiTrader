use ai_core::config::CONFIG;
use anyhow::Result;
use mcp_adapter::DemoArithmeticServer;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let config = CONFIG.clone();
    DemoArithmeticServer::new(config).serve_stdio().await
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("mcp-demo-server")
        .build("logs")
        .expect("Failed to create rolling file appender");

    let writer = std::io::stderr
        .with_max_level(tracing::Level::DEBUG)
        .and(file_appender);

    let subscriber = fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(writer)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}
