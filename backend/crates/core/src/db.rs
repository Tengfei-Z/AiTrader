use crate::config::DeepSeekConfig;
use anyhow::Result;
use postgres::{types::ToSql, Client, NoTls};
use serde::Deserialize;
use std::{env, fs, path::PathBuf};
use tracing::{info, warn};

const DEFAULT_DEEPSEEK_MODEL: &str = "deepseek-chat";
const DEFAULT_CONFIG_PATH: &str = "config/config.yaml";

fn database_url() -> Option<String> {
    load_url_from_config()
}

fn load_url_from_config() -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct DbConfig {
        url: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    struct FileConfig {
        db: Option<DbConfig>,
    }

    let config_path =
        env::var("AITRADER_CONFIG_PATH").unwrap_or_else(|_| DEFAULT_CONFIG_PATH.to_string());
    let mut path = PathBuf::from(&config_path);
    if !path.is_absolute() {
        if let Ok(current_dir) = env::current_dir() {
            path = current_dir.join(path);
        }
    }

    let contents = fs::read_to_string(path).ok()?;
    let config: FileConfig = serde_yaml::from_str(&contents).ok()?;
    config.db.and_then(|db| db.url).and_then(|url| {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub fn fetch_deepseek_credentials() -> Result<Option<DeepSeekConfig>> {
    let url = match database_url() {
        Some(url) => url,
        None => return Ok(None),
    };

    let mut client = match Client::connect(&url, NoTls) {
        Ok(client) => {
            info!("成功连接数据库，读取 DeepSeek 凭证");
            client
        }
        Err(err) => {
            warn!(%err, "连接数据库获取 DeepSeek 凭证失败");
            return Ok(None);
        }
    };

    ensure_deepseek_table(&mut client)?;

    let query = "SELECT api_key, endpoint, COALESCE(model, $1) AS model \
                 FROM deepseek_credentials ORDER BY updated_at DESC LIMIT 1";

    match client.query_opt(query, &[&DEFAULT_DEEPSEEK_MODEL]) {
        Ok(Some(row)) => {
            let api_key: String = row.get("api_key");
            let endpoint: String = row.get("endpoint");
            let model: String = row.get("model");
            info!("已从数据库加载 DeepSeek 凭证");
            Ok(Some(DeepSeekConfig {
                api_key,
                endpoint,
                model,
            }))
        }
        Ok(None) => {
            info!("数据库未找到 DeepSeek 凭证，将回退至环境变量");
            Ok(None)
        }
        Err(err) => {
            warn!(%err, "查询 DeepSeek 凭证失败，将回退至环境变量");
            Ok(None)
        }
    }
}

pub fn init_database() -> Result<()> {
    let url = match database_url() {
        Some(url) => url,
        None => {
            warn!("未在配置中找到数据库连接字符串，跳过初始化");
            return Ok(());
        }
    };

    let mut client = match Client::connect(&url, NoTls) {
        Ok(client) => {
            info!("数据库连接成功，开始初始化");
            client
        }
        Err(err) => {
            warn!(%err, "数据库初始化失败，无法连接");
            return Ok(());
        }
    };

    ensure_deepseek_table(&mut client)?;
    info!("数据库初始化完成");

    Ok(())
}

pub fn store_deepseek_credentials(config: &DeepSeekConfig) -> Result<()> {
    let url = match database_url() {
        Some(url) => url,
        None => return Ok(()),
    };

    let mut client = match Client::connect(&url, NoTls) {
        Ok(client) => {
            info!("成功连接数据库，写入 DeepSeek 凭证");
            client
        }
        Err(err) => {
            warn!(%err, "连接数据库写入 DeepSeek 凭证失败");
            return Ok(());
        }
    };

    if let Err(err) = ensure_deepseek_table(&mut client) {
        warn!(%err, "创建 DeepSeek 凭证表失败，将跳过写入");
        return Ok(());
    }

    let params: [&(dyn ToSql + Sync); 3] = [&config.api_key, &config.endpoint, &config.model];

    match client.execute(
        "INSERT INTO deepseek_credentials (api_key, endpoint, model, updated_at) \
         VALUES ($1, $2, $3, NOW())",
        &params,
    ) {
        Ok(_) => info!("DeepSeek 凭证已写入数据库"),
        Err(err) => warn!(%err, "写入 DeepSeek 凭证失败"),
    }

    Ok(())
}

fn ensure_deepseek_table(client: &mut Client) -> Result<()> {
    client.batch_execute(
        "CREATE TABLE IF NOT EXISTS deepseek_credentials (
            id          BIGSERIAL PRIMARY KEY,
            api_key     TEXT NOT NULL,
            endpoint    TEXT NOT NULL,
            model       TEXT NOT NULL DEFAULT 'deepseek-chat',
            updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );",
    )?;
    Ok(())
}
