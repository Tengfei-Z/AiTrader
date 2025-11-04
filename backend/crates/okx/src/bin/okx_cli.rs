use ai_core::config::{AppConfig, CONFIG};
use anyhow::Result;
use clap::{Parser, Subcommand};
use okx::OkxRestClient;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "okx-cli", about = "独立的 OKX API 测试工具", version)]
struct Cli {
    /// 是否使用模拟账户（默认开启）
    #[arg(long, default_value_t = true)]
    simulated: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 查询 OKX 当前服务器时间
    Time,
    /// 查询指定交易对的最新行情
    Ticker {
        /// 交易对标识，例如 BTC-USDT
        #[arg(long, short = 's')]
        symbol: String,
    },
    /// 查询K线数据
    Candles {
        /// 交易对标识，例如 BTC-USDT-SWAP
        #[arg(long, short = 's')]
        symbol: String,
        /// 时间周期，例如 3m, 5m, 1H
        #[arg(long, short = 't', default_value = "3m")]
        timeframe: String,
        /// 数据条数
        #[arg(long, short = 'l', default_value = "10")]
        limit: usize,
    },
    /// 查询账户权益与可用余额
    Balance,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let cli = Cli::parse();
    let config: &AppConfig = &CONFIG;
    let client = if cli.simulated {
        OkxRestClient::from_config_simulated(config)?
    } else {
        OkxRestClient::from_config(config)?
    };

    match cli.command {
        Command::Time => {
            let server_time = client.get_server_time().await?;
            println!("{}", server_time);
        }
        Command::Ticker { symbol } => {
            let ticker = client.get_ticker(&symbol).await?;
            println!("{}", serde_json::to_string_pretty(&ticker)?);
        }
        Command::Candles { symbol, timeframe, limit } => {
            let candles = client.get_candles(&symbol, &timeframe, Some(limit)).await?;
            println!("Retrieved {} candles:", candles.len());
            for (i, candle) in candles.iter().enumerate() {
                println!("  [{}] ts={}, o={}, h={}, l={}, c={}, v={:?}", 
                    i, candle.timestamp, candle.open, candle.high, 
                    candle.low, candle.close, candle.volume);
            }
        }
        Command::Balance => {
            let balance = client.get_account_balance().await?;
            println!("{}", serde_json::to_string_pretty(&balance)?);
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
