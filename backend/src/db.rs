use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};
use tokio_postgres::{Client, NoTls};
use tracing::{info, warn};
use uuid::Uuid;

const DEFAULT_CONFIG_PATH: &str = "config/config.yaml";
const DEFAULT_SCHEMA: &str = "aitrader";

#[derive(Debug, Deserialize)]
struct FileConfig {
    db: Option<DbSection>,
}

#[derive(Debug, Deserialize, Clone)]
struct DbSection {
    url: Option<String>,
    schema: Option<String>,
}

#[derive(Debug, Clone)]
struct DatabaseSettings {
    url: Option<String>,
    schema: String,
}

fn database_settings() -> DatabaseSettings {
    let mut settings = DatabaseSettings {
        url: None,
        schema: DEFAULT_SCHEMA.to_string(),
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

async fn connect_client(url: &str) -> Result<Client> {
    let (client, connection) = tokio_postgres::connect(url, NoTls).await?;
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            warn!(%err, "postgres connection error");
        }
    });
    Ok(client)
}

async fn run_migrations(client: &Client, schema: &str) -> Result<()> {
    for statement in migration_statements(schema) {
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

    Ok(())
}

fn migration_statements(schema: &str) -> Vec<String> {
    vec![
        "CREATE EXTENSION IF NOT EXISTS pgcrypto;".to_string(),
        format!("CREATE SCHEMA IF NOT EXISTS {schema};", schema = schema),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.strategies (
                id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                session_id      TEXT NOT NULL,
                summary         TEXT NOT NULL,
                content         TEXT NOT NULL DEFAULT '',
                role            TEXT NOT NULL DEFAULT 'assistant',
                tags            TEXT[] NOT NULL DEFAULT ARRAY[]::text[],
                confidence      NUMERIC(5, 2),
                created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
            );",
            schema = schema,
        ),
        format!(
            "ALTER TABLE {schema}.strategies
                ADD COLUMN IF NOT EXISTS content TEXT NOT NULL DEFAULT '',
                ADD COLUMN IF NOT EXISTS role TEXT NOT NULL DEFAULT 'assistant',
                ADD COLUMN IF NOT EXISTS tags TEXT[] NOT NULL DEFAULT ARRAY[]::text[];",
            schema = schema,
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.orders (
                id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                strategy_ids    UUID[] NOT NULL DEFAULT ARRAY[]::uuid[],
                symbol          TEXT NOT NULL,
                side            TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
                order_type      TEXT NOT NULL,
                price           NUMERIC(20, 8),
                size            NUMERIC(20, 8) NOT NULL,
                filled_size     NUMERIC(20, 8) NOT NULL DEFAULT 0,
                status          TEXT NOT NULL,
                leverage        NUMERIC(10, 2),
                confidence      NUMERIC(5, 2),
                metadata        JSONB NOT NULL DEFAULT '{{}}'::jsonb,
                created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
                closed_at       TIMESTAMPTZ
            );",
            schema = schema,
        ),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.initial_equities (
                id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                amount       NUMERIC(20, 8) NOT NULL,
                recorded_at  TIMESTAMPTZ NOT NULL DEFAULT now()
            );",
            schema = schema,
        ),
    ]
}

pub async fn init_database() -> Result<()> {
    let DatabaseSettings { url, schema } = database_settings();

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

    run_migrations(&client, schema.as_str()).await?;
    info!("数据库初始化完成");

    Ok(())
}

#[derive(Debug, Clone)]
pub struct StrategyMessageInsert {
    pub session_id: String,
    pub summary: String,
    pub content: String,
    pub role: String,
    pub tags: Vec<String>,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct StrategyMessageRecord {
    pub id: Uuid,
    pub summary: String,
    pub content: String,
    pub role: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

pub async fn insert_strategy_message(payload: StrategyMessageInsert) -> Result<()> {
    let DatabaseSettings { url, schema } = database_settings();

    let url = match url {
        Some(url) => url,
        None => {
            warn!("未配置数据库连接字符串，无法写入 strategy 记录");
            return Err(anyhow!("missing database url"));
        }
    };

    let client = match connect_client(&url).await {
        Ok(client) => client,
        Err(err) => {
            warn!(%err, "写入 strategy 记录时无法连接数据库");
            return Err(err.into());
        }
    };

    let sql = format!(
        "INSERT INTO {schema}.strategies (session_id, summary, content, role, tags, confidence)
         VALUES ($1, $2, $3, $4, $5, $6);",
        schema = schema,
    );

    client
        .execute(
            &sql,
            &[
                &payload.session_id,
                &payload.summary,
                &payload.content,
                &payload.role,
                &payload.tags,
                &payload.confidence,
            ],
        )
        .await
        .map(|_| ())
        .map_err(|err| {
            warn!(%err, "插入 strategy 记录失败");
            err.into()
        })
}

pub async fn fetch_strategy_messages(limit: i64) -> Result<Vec<StrategyMessageRecord>> {
    let DatabaseSettings { url, schema } = database_settings();

    let Some(url) = url else {
        warn!("未配置数据库连接字符串，跳过策略对话查询");
        return Ok(Vec::new());
    };

    let client = connect_client(&url).await?;
    let sql = format!(
        "SELECT id::text AS id_text, summary, content, role, tags, created_at
         FROM {schema}.strategies
         ORDER BY created_at DESC
         LIMIT $1;",
        schema = schema,
    );

    let rows = client.query(&sql, &[&limit]).await?;
    let mut records = Vec::with_capacity(rows.len());
    for row in rows {
        let id_raw: String = row.get("id_text");
        let id = match Uuid::parse_str(&id_raw) {
            Ok(uuid) => uuid,
            Err(err) => {
                warn!(%err, id = %id_raw, "failed to parse strategy uuid, using nil");
                Uuid::nil()
            }
        };

        records.push(StrategyMessageRecord {
            id,
            summary: row.get("summary"),
            content: row.get("content"),
            role: row.get("role"),
            tags: row.get::<_, Vec<String>>("tags"),
            created_at: row.get("created_at"),
        });
    }

    Ok(records)
}

pub async fn fetch_initial_equity() -> Result<Option<(f64, DateTime<Utc>)>> {
    let DatabaseSettings { url, schema } = database_settings();

    let url = match url {
        Some(url) => url,
        None => {
            warn!("无法读取数据库配置，跳过初始资金查询");
            return Ok(None);
        }
    };

    let client = connect_client(&url).await?;
    let sql = format!(
        "SELECT amount, recorded_at FROM {schema}.initial_equities ORDER BY recorded_at DESC LIMIT 1;",
        schema = schema
    );
    if let Some(row) = client.query_opt(&sql, &[]).await? {
        let amount: f64 = row.get("amount");
        let recorded_at: DateTime<Utc> = row.get("recorded_at");
        Ok(Some((amount, recorded_at)))
    } else {
        Ok(None)
    }
}

pub async fn insert_initial_equity(amount: f64) -> Result<()> {
    let DatabaseSettings { url, schema } = database_settings();

    let url = match url {
        Some(url) => url,
        None => {
            warn!("无法写入初始资金：未配置数据库 URL");
            return Err(anyhow!("missing database url"));
        }
    };

    let client = connect_client(&url).await?;
    let sql = format!(
        "INSERT INTO {schema}.initial_equities (amount) VALUES ($1);",
        schema = schema
    );
    client.execute(&sql, &[&amount]).await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct OrderHistoryRecord {
    pub symbol: String,
    pub side: String,
    pub price: Option<f64>,
    pub size: Option<f64>,
    pub leverage: Option<f64>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

pub async fn fetch_order_history(limit: Option<i64>) -> Result<Vec<OrderHistoryRecord>> {
    let DatabaseSettings { url, schema } = database_settings();

    let url = match url {
        Some(url) => url,
        None => {
            warn!("未配置数据库连接字符串，无法查询订单历史");
            return Err(anyhow!("missing database url"));
        }
    };

    let client = connect_client(&url).await?;

    let base_sql = format!(
        "SELECT
            symbol,
            side,
            price,
            size,
            leverage,
            metadata,
            created_at,
            closed_at
        FROM {schema}.orders
        WHERE closed_at IS NOT NULL
        ORDER BY closed_at DESC NULLS LAST",
        schema = schema,
    );

    let rows = if let Some(limit) = limit {
        let sql = format!("{base_sql} LIMIT $1");
        client.query(&sql, &[&limit]).await?
    } else {
        client.query(&base_sql, &[]).await?
    };

    let mut records = Vec::with_capacity(rows.len());
    for row in rows {
        records.push(OrderHistoryRecord {
            symbol: row.get::<_, String>("symbol"),
            side: row.get::<_, String>("side"),
            price: row.get::<_, Option<f64>>("price"),
            size: row.get::<_, Option<f64>>("size"),
            leverage: row.get::<_, Option<f64>>("leverage"),
            metadata: row.get::<_, Value>("metadata"),
            created_at: row.get::<_, DateTime<Utc>>("created_at"),
            closed_at: row.get::<_, Option<DateTime<Utc>>>("closed_at"),
        });
    }

    Ok(records)
}
