use ai_core::config::{AppConfig, CONFIG};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "trader-cli", about = "AiTrader 后端命令行工具", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// OKX API 冒烟测试
    #[cfg(feature = "okx")]
    Okx(OkxCommand),
    /// DeepSeek Function Call 测试
    #[cfg(feature = "deepseek")]
    Deepseek(DeepseekCommand),
    /// MCP 工具进程测试
    #[cfg(feature = "mcp")]
    Mcp(McpCommand),
}

#[derive(Debug, Parser)]
struct OkxCommand {
    #[command(subcommand)]
    action: OkxAction,
}

#[derive(Debug, Subcommand)]
enum OkxAction {
    /// 查询 OKX 服务器时间
    Time,
    /// 查询指定交易对的最新行情
    Ticker {
        /// 交易对标识，例如 BTC-USDT
        #[arg(long, short = 's')]
        symbol: String,
    },
}

#[cfg(feature = "deepseek")]
#[derive(Debug, Parser)]
struct DeepseekCommand {
    #[command(subcommand)]
    action: DeepseekAction,
}

#[cfg(feature = "deepseek")]
#[derive(Debug, Subcommand)]
enum DeepseekAction {
    /// 调用 DeepSeek 指定函数
    Call {
        /// 函数名称
        #[arg(long, short = 'f')]
        function: String,
        /// 参数（JSON 字符串）
        #[arg(long, short = 'a', default_value = "null")]
        arguments: String,
        /// 附加元数据（JSON 字符串）
        #[arg(long, default_value = "null")]
        metadata: String,
    },
    /// 发送聊天消息，请求 DeepSeek 评价行情
    Chat {
        /// 用户消息
        #[arg(
            long,
            short = 'p',
            default_value = "请评价一下当前的 BTC 行情，并给出风险提示。"
        )]
        prompt: String,
    },
    /// 调用 DeepSeek 获取当前账户信息
    AccountState,
}

#[cfg(feature = "mcp")]
#[derive(Debug, Parser)]
struct McpCommand {
    #[command(subcommand)]
    action: McpAction,
}

#[cfg(feature = "mcp")]
#[derive(Debug, Subcommand)]
enum McpAction {
    /// 启动 MCP 工具并发送请求
    Send {
        /// 工具名称
        #[arg(long, short = 't')]
        tool: String,
        /// JSON 字符串作为 payload
        #[arg(long, short = 'p', default_value = "null")]
        payload: String,
        /// 是否跳过等待响应
        #[arg(long = "no-wait-response", default_value_t = false)]
        no_wait_response: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;
    let cli = Cli::parse();

    match cli.command {
        #[cfg(feature = "okx")]
        Commands::Okx(cmd) => handle_okx(cmd).await?,
        #[cfg(feature = "deepseek")]
        Commands::Deepseek(cmd) => handle_deepseek(cmd).await?,
        #[cfg(feature = "mcp")]
        Commands::Mcp(cmd) => handle_mcp(cmd).await?,
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

#[cfg(feature = "okx")]
async fn handle_okx(cmd: OkxCommand) -> Result<()> {
    use okx::OkxRestClient;

    let config: &AppConfig = &CONFIG;
    let client = OkxRestClient::from_config(config)?;

    match cmd.action {
        OkxAction::Time => {
            let server_time = client.get_server_time().await?;
            println!("{}", server_time);
        }
        OkxAction::Ticker { symbol } => {
            let ticker = client.get_ticker(&symbol).await?;
            println!("{}", serde_json::to_string_pretty(&ticker)?);
        }
    }

    Ok(())
}

#[cfg(feature = "deepseek")]
async fn handle_deepseek(cmd: DeepseekCommand) -> Result<()> {
    use deepseek::{DeepSeekClient, FunctionCallRequest, FunctionCaller};
    use serde_json::json;

    let config: &AppConfig = &CONFIG;
    let client = DeepSeekClient::from_app_config(config)?;

    match cmd.action {
        DeepseekAction::Call {
            function,
            arguments,
            metadata,
        } => {
            let request = FunctionCallRequest {
                function,
                arguments: serde_json::from_str(&arguments).context("arguments 不是合法 JSON")?,
                metadata: serde_json::from_str(&metadata).context("metadata 不是合法 JSON")?,
            };

            let response = client.call_function(request).await?;
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        DeepseekAction::Chat { prompt } => {
            let reply = client.chat_completion(&prompt).await?;
            println!("{}", reply);
        }
        DeepseekAction::AccountState => {
            let parameters_schema = json!({
                "type": "object",
                "properties": {
                    "include_positions": {
                        "type": "boolean",
                        "default": true
                    },
                    "include_history": {
                        "type": "boolean",
                        "default": false
                    },
                    "include_performance": {
                        "type": "boolean",
                        "default": false
                    }
                },
                "required": ["include_positions", "include_history", "include_performance"],
                "additionalProperties": false
            });

            let request = FunctionCallRequest {
                function: "get_account_state".to_string(),
                arguments: json!({
                    "include_positions": true,
                    "include_history": true,
                    "include_performance": true
                }),
                metadata: json!({
                    "source": "trader-cli",
                    "description": "Retrieve aggregated OKX account balances, performance indicators, and open positions.",
                    "parameters": parameters_schema,
                    "system_prompt": "You are an assistant that relays trading account requests. When asked to get account information, always call the provided tool with proper JSON arguments."
                }),
            };

            let response = client.call_function(request).await?;
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
    }

    Ok(())
}

#[cfg(feature = "mcp")]
async fn handle_mcp(cmd: McpCommand) -> Result<()> {
    use mcp_adapter::{McpProcessHandle, McpRequest};

    let config: &AppConfig = &CONFIG;

    match cmd.action {
        McpAction::Send {
            tool,
            payload,
            no_wait_response,
        } => {
            let mut handle = McpProcessHandle::spawn_from_app_config(config).await?;
            let request = McpRequest {
                tool,
                payload: serde_json::from_str(&payload).context("payload 不是合法 JSON")?,
            };

            handle.send(request).await?;

            if no_wait_response {
                println!("请求已发送，未等待响应。");
            } else if let Some(response) = handle.read_stdout().await? {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("MCP 进程未返回数据。");
            }
        }
    }

    Ok(())
}
