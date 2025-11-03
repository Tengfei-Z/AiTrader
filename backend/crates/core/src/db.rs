use crate::config::DeepSeekConfig;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::{
    collections::HashSet,
    env, fs,
    future::Future,
    path::{Path, PathBuf},
};
use tokio::runtime::{Builder, Handle};
use tokio_postgres::{types::ToSql, Client, NoTls};
use tracing::{info, warn};

const DEFAULT_DEEPSEEK_MODEL: &str = "deepseek-chat";
const DEFAULT_CONFIG_PATH: &str = "config/config.yaml";
const DEFAULT_SCHEMA: &str = "aitrader";
const DEFAULT_SETTINGS_TABLE: &str = "settings";

#[derive(Debug, Deserialize)]
struct FileConfig {
    db: Option<DbSection>,
}

#[derive(Debug, Deserialize, Clone)]
struct DbSection {
    url: Option<String>,
    schema: Option<String>,
    settings_table: Option<String>,
    #[serde(alias = "deepseek_table")]
    legacy_deepseek_table: Option<String>,
}

#[derive(Debug, Clone)]
struct DatabaseSettings {
    url: Option<String>,
    schema: String,
    settings_table: String,
}

fn database_settings() -> DatabaseSettings {
    let mut settings = DatabaseSettings {
        url: None,
        schema: DEFAULT_SCHEMA.to_string(),
        settings_table: DEFAULT_SETTINGS_TABLE.to_string(),
    };

    if let Some(db_section) = load_db_section_from_config() {
        if let Some(url) = db_section.url.as_ref().and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }) {
            settings.url = Some(url);
        }

        if let Some(schema) = db_section.schema.as_ref().and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }) {
            settings.schema = schema;
        }

        let table_override = db_section
            .settings_table
            .as_ref()
            .or(db_section.legacy_deepseek_table.as_ref());

        if let Some(table) = table_override.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }) {
            settings.settings_table = table;
        }
    }

    settings
}

fn load_db_section_from_config() -> Option<DbSection> {
    let config_path =
        env::var("AITRADER_CONFIG_PATH").unwrap_or_else(|_| DEFAULT_CONFIG_PATH.to_string());

    for candidate in candidate_paths(&config_path) {
        let path = candidate.clone();
        if let Some(config) = read_config(candidate) {
            if let Some(db) = config.db {
                info!(path = %path.display(), "Loaded database configuration from file");
                return Some(db);
            }
        }
    }

    warn!(
        path = %config_path,
        "Database configuration not found in any candidate path"
    );
    None
}

fn read_config(path: PathBuf) -> Option<FileConfig> {
    if !path.exists() {
        return None;
    }

    let contents = fs::read_to_string(&path).ok()?;
    serde_yaml::from_str(&contents).ok()
}

fn candidate_paths(config_path: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let target = PathBuf::from(config_path);

    if target.is_absolute() {
        candidates.push(target);
        return candidates;
    }

    if let Ok(repo_root) = env::var("AITRADER_REPO_ROOT") {
        let base = PathBuf::from(repo_root);
        push_candidate(&base.join(config_path), &mut candidates, &mut seen);
    }

    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let base = PathBuf::from(manifest_dir);
        for ancestor in base.ancestors() {
            push_candidate(
                &PathBuf::from(ancestor).join(config_path),
                &mut candidates,
                &mut seen,
            );
        }
    }

    if let Ok(current_dir) = env::current_dir() {
        for ancestor in current_dir.ancestors() {
            push_candidate(
                &PathBuf::from(ancestor).join(config_path),
                &mut candidates,
                &mut seen,
            );
        }
    }

    // 常见的 repo 相对路径，避免遗漏
    push_candidate(
        &Path::new("..").join(config_path),
        &mut candidates,
        &mut seen,
    );
    push_candidate(
        &Path::new("../..").join(config_path),
        &mut candidates,
        &mut seen,
    );

    candidates
}

fn push_candidate(path: &Path, candidates: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>) {
    let canonical = if path.is_absolute() {
        path.to_path_buf()
    } else {
        PathBuf::from(path)
    };

    if seen.insert(canonical.clone()) {
        candidates.push(canonical);
    }
}

fn block_on_db<F, T>(future: F) -> Result<T>
where
    F: Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    if let Ok(handle) = Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to build Tokio runtime for database operation")?;
        Ok(runtime.block_on(future)?)
    }
}

async fn connect_client(url: &str) -> Result<Client> {
    let (client, connection) = tokio_postgres::connect(url, NoTls).await?;
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            warn!(%err, "postgres connection error");
        }
    });
    Ok(client)
}

async fn run_migrations(client: &Client, schema: &str, settings_table: &str) -> Result<()> {
    for statement in migration_statements(schema, settings_table) {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }

        match client.batch_execute(trimmed).await {
            Ok(_) => {}
            Err(err) if trimmed.starts_with("CREATE EXTENSION IF NOT EXISTS pgcrypto") => {
                if let Some(db_err) = err.as_db_error() {
                    let code = db_err.code().code();
                    warn!(
                        code = code,
                        message = db_err.message(),
                        detail = db_err.detail().unwrap_or_default(),
                        hint = db_err.hint().unwrap_or_default(),
                        "failed to create pgcrypto extension, continuing"
                    );
                } else {
                    warn!(?err, "failed to create pgcrypto extension, continuing");
                }
            }
            Err(err) => {
                if let Some(db_err) = err.as_db_error() {
                    let code = db_err.code().code();
                    warn!(
                        stmt = trimmed,
                        code = code,
                        message = db_err.message(),
                        detail = db_err.detail().unwrap_or_default(),
                        hint = db_err.hint().unwrap_or_default(),
                        "database migration statement failed"
                    );
                } else {
                    warn!(?err, stmt = trimmed, "database migration statement failed");
                }
                return Err(err.into());
            }
        }
    }

    normalize_settings_table(client, schema, settings_table).await?;
    normalize_performance_snapshots(client, schema).await?;
    migrate_legacy_deepseek_table(client, schema, settings_table).await?;
    Ok(())
}

fn migration_statements(schema: &str, settings_table: &str) -> Vec<String> {
    vec![
        "CREATE EXTENSION IF NOT EXISTS pgcrypto;".to_string(),
        format!("CREATE SCHEMA IF NOT EXISTS {schema};", schema = schema),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.accounts (
                id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                external_id     TEXT NOT NULL UNIQUE,
                mode            TEXT NOT NULL CHECK (mode IN ('live', 'simulated')),
                status          TEXT NOT NULL DEFAULT 'active',
                created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
            );",
            schema = schema,
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.balance_snapshots (
                id              BIGSERIAL PRIMARY KEY,
                account_id      UUID NOT NULL REFERENCES {schema}.accounts (id),
                available_usdt  NUMERIC(24, 8) NOT NULL,
                locked_usdt     NUMERIC(24, 8) NOT NULL DEFAULT 0,
                as_of           TIMESTAMPTZ NOT NULL,
                created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE (account_id, as_of)
            );",
            schema = schema,
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.{settings_table} (
                id                  BIGSERIAL PRIMARY KEY,
                deepseek_api_key    TEXT NOT NULL,
                deepseek_endpoint   TEXT NOT NULL,
                deepseek_model      TEXT NOT NULL DEFAULT '{default_model}',
                updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
            );",
            schema = schema,
            settings_table = settings_table,
            default_model = DEFAULT_DEEPSEEK_MODEL,
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.orders (
                id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                account_id      UUID NOT NULL REFERENCES {schema}.accounts (id),
                symbol          TEXT NOT NULL,
                side            TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
                order_type      TEXT NOT NULL,
                price           NUMERIC(20, 8),
                size            NUMERIC(20, 8) NOT NULL,
                filled_size     NUMERIC(20, 8) NOT NULL DEFAULT 0,
                status          TEXT NOT NULL,
                leverage        NUMERIC(10, 2),
                confidence      NUMERIC(5, 2),
                tool_call_id    UUID,
                created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
            );",
            schema = schema,
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.fills (
                id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                account_id      UUID NOT NULL REFERENCES {schema}.accounts (id),
                order_id        UUID NOT NULL REFERENCES {schema}.orders (id),
                symbol          TEXT NOT NULL,
                side            TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
                price           NUMERIC(20, 8) NOT NULL,
                size            NUMERIC(20, 8) NOT NULL,
                fee_usdt        NUMERIC(20, 8) NOT NULL DEFAULT 0,
                pnl_usdt        NUMERIC(24, 8),
                confidence      NUMERIC(5, 2),
                timestamp       TIMESTAMPTZ NOT NULL
            );",
            schema = schema,
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.positions_open (
                id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                account_id              UUID NOT NULL REFERENCES {schema}.accounts (id),
                symbol                  TEXT NOT NULL,
                side                    TEXT NOT NULL,
                quantity                NUMERIC(20, 8) NOT NULL,
                avg_entry_price         NUMERIC(20, 8),
                leverage                NUMERIC(10, 2),
                margin_usdt             NUMERIC(24, 8),
                liquidation_price       NUMERIC(20, 8),
                unrealized_pnl_usdt     NUMERIC(24, 8),
                exit_plan               JSONB DEFAULT '{{}}'::jsonb,
                opened_at               TIMESTAMPTZ,
                updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE (account_id, symbol, side)
            );",
            schema = schema,
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.positions_closed (
                id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                account_id              UUID NOT NULL REFERENCES {schema}.accounts (id),
                symbol                  TEXT NOT NULL,
                side                    TEXT NOT NULL,
                quantity                NUMERIC(20, 8) NOT NULL,
                entry_price             NUMERIC(20, 8),
                exit_price              NUMERIC(20, 8),
                realized_pnl_usdt       NUMERIC(24, 8),
                holding_minutes         NUMERIC(14, 4),
                average_confidence      NUMERIC(5, 2),
                entry_time              TIMESTAMPTZ,
                exit_time               TIMESTAMPTZ NOT NULL,
                created_at              TIMESTAMPTZ NOT NULL DEFAULT now()
            );",
            schema = schema,
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.mcp_tool_calls (
                id                  UUID PRIMARY KEY,
                account_id          UUID REFERENCES {schema}.accounts (id),
                tool_name           TEXT NOT NULL,
                request_payload     JSONB NOT NULL,
                response_payload    JSONB,
                status              TEXT NOT NULL DEFAULT 'success',
                latency_ms          INTEGER,
                created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
            );",
            schema = schema,
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.market_snapshots (
                id              BIGSERIAL PRIMARY KEY,
                symbol          TEXT NOT NULL,
                timeframe       TEXT NOT NULL,
                as_of           TIMESTAMPTZ NOT NULL,
                price           NUMERIC(20, 8),
                ema20           NUMERIC(20, 8),
                ema50           NUMERIC(20, 8),
                macd            NUMERIC(20, 8),
                rsi7            NUMERIC(8, 4),
                rsi14           NUMERIC(8, 4),
                funding_rate    NUMERIC(10, 8),
                open_interest   NUMERIC(24, 4),
                created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE (symbol, timeframe, as_of)
            );",
            schema = schema,
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.performance_snapshots (
                id                      BIGSERIAL PRIMARY KEY,
                account_id              UUID NOT NULL REFERENCES {schema}.accounts (id),
                window_name             TEXT NOT NULL,
                sharpe_ratio            NUMERIC(10, 6),
                win_rate                NUMERIC(6, 4),
                average_leverage        NUMERIC(10, 4),
                average_confidence      NUMERIC(5, 2),
                biggest_win_usdt        NUMERIC(24, 8),
                biggest_loss_usdt       NUMERIC(24, 8),
                hold_ratio_long         NUMERIC(6, 4),
                hold_ratio_short        NUMERIC(6, 4),
                hold_ratio_flat         NUMERIC(6, 4),
                updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE (account_id, window_name)
            );",
            schema = schema,
        ),
    ]
}

async fn normalize_settings_table(
    client: &Client,
    schema: &str,
    settings_table: &str,
) -> Result<()> {
    let columns_query = r#"
        SELECT column_name
        FROM information_schema.columns
        WHERE table_schema = $1
          AND table_name = $2
    "#;

    let rows = client
        .query(columns_query, &[&schema, &settings_table])
        .await?;
    let columns: HashSet<String> = rows
        .into_iter()
        .map(|row| row.get::<_, String>("column_name"))
        .collect();

    let mut renames = Vec::new();

    if columns.contains("api_key") && !columns.contains("deepseek_api_key") {
        renames.push(("api_key", "deepseek_api_key"));
    }

    if columns.contains("endpoint") && !columns.contains("deepseek_endpoint") {
        renames.push(("endpoint", "deepseek_endpoint"));
    }

    if columns.contains("model") && !columns.contains("deepseek_model") {
        renames.push(("model", "deepseek_model"));
    }

    for (from, to) in renames {
        let sql = format!(
            "ALTER TABLE {schema}.{table} RENAME COLUMN {from} TO {to}",
            schema = schema,
            table = settings_table,
            from = from,
            to = to
        );
        client.execute(&sql, &[]).await?;
    }

    Ok(())
}

async fn normalize_performance_snapshots(client: &Client, schema: &str) -> Result<()> {
    let columns_query = r#"
        SELECT column_name
        FROM information_schema.columns
        WHERE table_schema = $1
          AND table_name = $2
    "#;

    let rows = client
        .query(columns_query, &[&schema, &"performance_snapshots"])
        .await?;
    let columns: HashSet<String> = rows
        .into_iter()
        .map(|row| row.get::<_, String>("column_name"))
        .collect();

    if columns.contains("window") && !columns.contains("window_name") {
        let sql = format!(
            "ALTER TABLE {schema}.performance_snapshots RENAME COLUMN window TO window_name",
            schema = schema,
        );
        client.execute(&sql, &[]).await?;
    }

    Ok(())
}

async fn migrate_legacy_deepseek_table(
    client: &Client,
    schema: &str,
    settings_table: &str,
) -> Result<()> {
    if settings_table == "deepseek_credentials" {
        return Ok(());
    }

    let legacy_table = format!("{schema}.deepseek_credentials");
    let check_sql = "SELECT to_regclass($1)::text AS table_name";
    let legacy_exists = match client.query_opt(check_sql, &[&legacy_table]).await? {
        Some(row) => row
            .get::<_, Option<String>>("table_name")
            .map(|name| !name.is_empty())
            .unwrap_or(false),
        None => false,
    };

    if !legacy_exists {
        return Ok(());
    }

    let migrate_sql = format!(
        "INSERT INTO {schema}.{settings_table} (deepseek_api_key, deepseek_endpoint, deepseek_model, updated_at)
         SELECT api_key, endpoint, model, updated_at
         FROM {schema}.deepseek_credentials legacy
         WHERE NOT EXISTS (
             SELECT 1 FROM {schema}.{settings_table} s
             WHERE s.deepseek_api_key = legacy.api_key
               AND s.deepseek_endpoint = legacy.endpoint
               AND s.deepseek_model = legacy.model
         );",
        schema = schema,
        settings_table = settings_table
    );

    let _ = client.execute(&migrate_sql, &[]).await?;
    Ok(())
}

pub async fn fetch_deepseek_credentials_async() -> Result<Option<DeepSeekConfig>> {
    let DatabaseSettings {
        url,
        schema,
        settings_table,
    } = database_settings();

    let url = match url {
        Some(url) => url,
        None => {
            warn!("未在配置中找到数据库连接字符串，将跳过 DeepSeek 凭证读取");
            return Ok(None);
        }
    };

    let client = match connect_client(&url).await {
        Ok(client) => {
            info!("成功连接数据库，读取 DeepSeek 凭证");
            client
        }
        Err(err) => {
            warn!(%err, "连接数据库获取 DeepSeek 凭证失败");
            return Ok(None);
        }
    };

    run_migrations(&client, schema.as_str(), settings_table.as_str()).await?;

    let query = format!(
        "SELECT deepseek_api_key, deepseek_endpoint, COALESCE(deepseek_model, $1) AS deepseek_model \
         FROM {schema}.{table} ORDER BY updated_at DESC LIMIT 1",
        schema = schema,
        table = settings_table
    );

    match client.query_opt(&query, &[&DEFAULT_DEEPSEEK_MODEL]).await {
        Ok(Some(row)) => {
            let api_key: String = row.get("deepseek_api_key");
            let endpoint: String = row.get("deepseek_endpoint");
            let model: String = row.get("deepseek_model");
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

pub fn fetch_deepseek_credentials() -> Result<Option<DeepSeekConfig>> {
    block_on_db(fetch_deepseek_credentials_async())
}

pub async fn init_database() -> Result<()> {
    let DatabaseSettings {
        url,
        schema,
        settings_table,
    } = database_settings();

    let url = match url {
        Some(url) => url,
        None => {
            warn!("未在配置中找到数据库连接字符串，跳过初始化");
            return Ok(());
        }
    };

    let client = match connect_client(&url).await {
        Ok(client) => {
            info!("数据库连接成功，开始初始化");
            client
        }
        Err(err) => {
            warn!(%err, "数据库初始化失败，无法连接");
            return Ok(());
        }
    };

    run_migrations(&client, schema.as_str(), settings_table.as_str()).await?;
    info!("数据库初始化完成");

    Ok(())
}

async fn store_deepseek_credentials_inner(config: DeepSeekConfig) -> Result<()> {
    let DatabaseSettings {
        url,
        schema,
        settings_table,
    } = database_settings();

    let url = match url {
        Some(url) => url,
        None => {
            warn!("未在配置中找到数据库连接字符串，将跳过 DeepSeek 凭证写入");
            return Ok(());
        }
    };

    let client = match connect_client(&url).await {
        Ok(client) => {
            info!("成功连接数据库，写入 DeepSeek 凭证");
            client
        }
        Err(err) => {
            warn!(%err, "连接数据库写入 DeepSeek 凭证失败");
            return Ok(());
        }
    };

    if let Err(err) = run_migrations(&client, schema.as_str(), settings_table.as_str()).await {
        warn!(%err, "创建 DeepSeek 凭证表失败，将跳过写入");
        return Ok(());
    }

    let params: [&(dyn ToSql + Sync); 3] = [&config.api_key, &config.endpoint, &config.model];

    let insert = format!(
        "INSERT INTO {schema}.{table} (deepseek_api_key, deepseek_endpoint, deepseek_model, updated_at) \
         VALUES ($1, $2, $3, NOW())",
        schema = schema,
        table = settings_table
    );

    match client.execute(&insert, &params).await {
        Ok(_) => info!("DeepSeek 凭证已写入数据库"),
        Err(err) => {
            if let Some(db_err) = err.as_db_error() {
                let code = db_err.code().code();
                warn!(
                    code = code,
                    message = db_err.message(),
                    detail = db_err.detail().unwrap_or_default(),
                    hint = db_err.hint().unwrap_or_default(),
                    "写入 DeepSeek 凭证失败"
                );
            } else {
                warn!(?err, "写入 DeepSeek 凭证失败");
            }
        }
    }

    Ok(())
}

pub async fn store_deepseek_credentials_async(config: &DeepSeekConfig) -> Result<()> {
    store_deepseek_credentials_inner(config.clone()).await
}

pub fn store_deepseek_credentials(config: &DeepSeekConfig) -> Result<()> {
    block_on_db(store_deepseek_credentials_inner(config.clone()))
}
