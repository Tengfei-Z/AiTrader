use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};
use tokio_postgres::{Client, NoTls};
use tracing::{info, warn};

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
                confidence      NUMERIC(5, 2),
                created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
            );",
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

pub async fn insert_strategy_summary(
    session_id: &str,
    summary: &str,
    confidence: Option<f64>,
) -> Result<()> {
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

    let session_owned = session_id.to_owned();
    let summary_owned = summary.to_owned();

    if let Some(conf) = confidence {
        let sql = format!(
            "INSERT INTO {schema}.strategies (session_id, summary, confidence) VALUES ($1, $2, $3);",
            schema = schema,
        );
        client
            .execute(&sql, &[&session_owned, &summary_owned, &conf])
            .await
            .map(|_| ())
            .map_err(|err| {
                warn!(%err, "插入 strategy 记录失败");
                err.into()
            })
    } else {
        let sql = format!(
            "INSERT INTO {schema}.strategies (session_id, summary) VALUES ($1, $2);",
            schema = schema,
        );
        client
            .execute(&sql, &[&session_owned, &summary_owned])
            .await
            .map(|_| ())
            .map_err(|err| {
                warn!(%err, "插入 strategy 记录失败");
                err.into()
            })
    }
}
