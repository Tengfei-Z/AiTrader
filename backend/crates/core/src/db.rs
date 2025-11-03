use crate::config::DeepSeekConfig;
use anyhow::Result;
use postgres::{types::ToSql, Client, NoTls};
use std::env;
use tracing::warn;

const DEFAULT_DEEPSEEK_MODEL: &str = "deepseek-chat";

fn database_url() -> Option<String> {
    env::var("DATABASE_URL").ok().and_then(|value| {
        let trimmed = value.trim();
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
        Ok(client) => client,
        Err(err) => {
            warn!(%err, "连接数据库获取 DeepSeek 凭证失败");
            return Ok(None);
        }
    };

    let query = "SELECT api_key, endpoint, COALESCE(model, $1) AS model \
                 FROM deepseek_credentials ORDER BY updated_at DESC LIMIT 1";

    match client.query_opt(query, &[&DEFAULT_DEEPSEEK_MODEL]) {
        Ok(Some(row)) => {
            let api_key: String = row.get("api_key");
            let endpoint: String = row.get("endpoint");
            let model: String = row.get("model");
            Ok(Some(DeepSeekConfig {
                api_key,
                endpoint,
                model,
            }))
        }
        Ok(None) => Ok(None),
        Err(err) => {
            warn!(%err, "查询 DeepSeek 凭证失败，将回退至环境变量");
            Ok(None)
        }
    }
}

pub fn store_deepseek_credentials(config: &DeepSeekConfig) -> Result<()> {
    let url = match database_url() {
        Some(url) => url,
        None => return Ok(()),
    };

    let mut client = match Client::connect(&url, NoTls) {
        Ok(client) => client,
        Err(err) => {
            warn!(%err, "连接数据库写入 DeepSeek 凭证失败");
            return Ok(());
        }
    };

    let params: [&dyn ToSql; 3] = [&config.api_key, &config.endpoint, &config.model];

    if let Err(err) = client.execute(
        "INSERT INTO deepseek_credentials (api_key, endpoint, model, updated_at) \
         VALUES ($1, $2, $3, NOW())",
        &params,
    ) {
        warn!(%err, "写入 DeepSeek 凭证失败");
    }

    Ok(())
}
