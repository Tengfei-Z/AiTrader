use ai_core::config::{AppConfig, CONFIG};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use deepseek::DeepSeekClient; // ✅ 只需要 DeepSeekClient
use serde_json::Value;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "deepseek-cli",
    about = "DeepSeek Autonomous Analyze 测试工具",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 自主分析（模型可自由选择是否/哪个工具）
    Analyze {
        /// system prompt
        #[arg(long, default_value = "你是一个专业的加密货币交易助手。若需要实时数据或交易操作，请调用相应工具。")]
        system: String,
        /// 用户问题（user prompt）
        #[arg(long, short = 'p', default_value = "请评价一下当前的 BTC 行情，给出风险提示。")]
        prompt: String,
    },

    /// 发送一条简单聊天消息（不使用工具）
    Chat {
        #[arg(long, short = 'p', default_value = "请评价一下当前的 BTC 行情，给出风险提示。")]
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
        Command::Analyze { system, prompt } => {
            let resp = client.autonomous_analyze(&system, &prompt).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
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
    ).is_err() {
        // tracing already initialised; ignore.
    }
    Ok(())
}

fn parse_json(input: &str, field: &str) -> Result<Value> {
    serde_json::from_str(input).with_context(|| format!("{} 字段不是合法的 JSON: {}", field, input))
}
