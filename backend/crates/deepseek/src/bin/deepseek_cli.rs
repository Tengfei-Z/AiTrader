use ai_core::config::{AppConfig, CONFIG};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use deepseek::{DeepSeekClient, FunctionCallRequest, FunctionCaller};
use serde_json::Value;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "deepseek-cli",
    about = "DeepSeek Function Call 测试工具",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 调用指定函数
    Call {
        /// 函数名称
        #[arg(long, short = 'f')]
        function: String,
        /// 参数（JSON 字符串），例如 '{"symbol":"BTC-USDT"}'
        #[arg(long, short = 'a', default_value = "null")]
        arguments: String,
        /// 附加元数据（JSON 字符串）
        #[arg(long, default_value = "null")]
        metadata: String,
    },
    /// 发送一条简单聊天消息
    Chat {
        /// 用户消息内容
        #[arg(
            long,
            short = 'p',
            default_value = "请评价一下当前的 BTC 行情，给出风险提示。"
        )]
        prompt: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let cli = Cli::parse();
    let app_config: &AppConfig = &CONFIG;
    let client = DeepSeekClient::from_app_config(app_config)?;

    match cli.command {
        Command::Call {
            function,
            arguments,
            metadata,
        } => {
            let request = FunctionCallRequest {
                function,
                arguments: parse_json(&arguments, "arguments")?,
                metadata: parse_json(&metadata, "metadata")?,
            };

            let response = client.call_function(request).await?;
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        Command::Chat { prompt } => {
            let reply = client.chat_completion(&prompt).await?;
            println!("{}", reply);
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
        // tracing already initialised; ignore.
    }
    Ok(())
}

fn parse_json(input: &str, field: &str) -> Result<Value> {
    serde_json::from_str(input).with_context(|| format!("{} 字段不是合法的 JSON: {}", field, input))
}
