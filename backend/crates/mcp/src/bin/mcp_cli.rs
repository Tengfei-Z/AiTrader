use ai_core::config::{AppConfig, CONFIG};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mcp_adapter::{process::McpProcessHandle, types::McpRequest};
use serde_json::Value;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "mcp-cli", about = "MCP 工具进程调试器", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 启动 MCP 进程并发送请求
    Send {
        /// 工具名称
        #[arg(long, short = 't')]
        tool: String,
        /// JSON 内容
        #[arg(long, short = 'p', default_value = "null")]
        payload: String,
        /// 发送后是否等待一条响应
        #[arg(long = "no-wait-response", default_value_t = false)]
        no_wait_response: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let cli = Cli::parse();
    let config: &AppConfig = &CONFIG;

    match cli.command {
        Command::Send {
            tool,
            payload,
            no_wait_response,
        } => {
            let mut handle = McpProcessHandle::spawn_from_app_config(config).await?;
            let request = McpRequest {
                tool,
                payload: parse_json(&payload, "payload")?,
            };

            handle.send(request).await?;

            if !no_wait_response {
                if let Some(response) = handle.read_stdout().await? {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                } else {
                    println!("MCP 进程未返回数据");
                }
            }
        }
    }

    Ok(())
}

fn init_tracing() -> Result<()> {
    if tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .finish(),
    )
    .is_err()
    {
        // tracing 已经初始化则忽略
    }
    Ok(())
}

fn parse_json(input: &str, field: &str) -> Result<Value> {
    serde_json::from_str(input).with_context(|| format!("{} 字段不是合法 JSON: {}", field, input))
}
