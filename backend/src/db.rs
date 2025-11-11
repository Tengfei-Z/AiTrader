//! 数据库操作模块
//!
//! 提供 PostgreSQL 数据库的连接、迁移和 CRUD 操作功能
//! - 数据库初始化和表结构迁移（migration）
//! - 策略消息的存储和查询
//! - 订单历史记录管理
//! - 账户余额快照记录
//! - 初始资金记录

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};
use tokio_postgres::{Client, NoTls};
use tracing::{info, warn};
use uuid::Uuid;

/// 默认配置文件路径
const DEFAULT_CONFIG_PATH: &str = "config/config.yaml";
/// 默认数据库 schema 名称
const DEFAULT_SCHEMA: &str = "aitrader";

/// 全局配置缓存（只读取一次）
static DB_CONFIG_CACHE: OnceLock<Option<DbSection>> = OnceLock::new();

/// 配置文件结构（顶层）
#[derive(Debug, Deserialize)]
struct FileConfig {
    db: Option<DbSection>,
}

/// 数据库配置段
#[derive(Debug, Deserialize, Clone)]
struct DbSection {
    /// 数据库连接字符串（如 postgresql://user:pass@localhost/dbname）
    url: Option<String>,
    /// 数据库 schema 名称（默认为 aitrader）
    schema: Option<String>,
}

/// 内部使用的数据库设置
#[derive(Debug, Clone)]
struct DatabaseSettings {
    /// 数据库连接 URL
    url: Option<String>,
    /// 数据库 schema 名称
    schema: String,
}

/// 从配置文件加载数据库设置（带缓存）
///
/// 读取 config.yaml 中的数据库配置，包括连接 URL 和 schema 名称
///
/// **性能优化：**
/// - 使用 `OnceLock` 缓存配置，首次调用时读取文件，之后直接使用缓存
/// - 避免每次数据库操作都重新读取配置文件
fn database_settings() -> DatabaseSettings {
    let mut settings = DatabaseSettings {
        url: None,
        schema: DEFAULT_SCHEMA.to_string(),
    };

    // 优先使用环境变量（local dev-friendly）
    if let Some(env_url) = env::var("DATABASE_URL")
        .ok()
        .and_then(|value| parse_non_empty(&value))
    {
        settings.url = Some(env_url);
    }

    if let Some(env_schema) = env::var("DATABASE_SCHEMA")
        .ok()
        .and_then(|value| parse_non_empty(&value))
    {
        settings.schema = env_schema;
    }

    if settings.url.is_some() {
        return settings;
    }

    // 使用缓存的配置（第一次调用时会读取文件并缓存）
    let db_section = DB_CONFIG_CACHE.get_or_init(|| load_db_section_from_config());

    if let Some(db_section) = db_section {
        // 读取数据库连接 URL（去除空白字符）
        if let Some(url) = db_section
            .url
            .as_ref()
            .and_then(|value| parse_non_empty(value.as_str()))
        {
            settings.url = Some(url);
        }

        // 读取 schema 名称（去除空白字符）
        if let Some(schema) = db_section
            .schema
            .as_ref()
            .and_then(|value| parse_non_empty(value.as_str()))
        {
            settings.schema = schema;
        }
    }

    settings
}

/// 从配置文件中加载数据库配置段
///
/// 会在多个候选路径中搜索配置文件（如 config/config.yaml）
fn load_db_section_from_config() -> Option<DbSection> {
    let config_path =
        env::var("AITRADER_CONFIG_PATH").unwrap_or_else(|_| DEFAULT_CONFIG_PATH.to_string());

    // 在多个候选路径中搜索配置文件
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

/// 读取 YAML 配置文件
///
/// 解析 YAML 文件并返回配置结构
fn read_config(path: PathBuf) -> Option<FileConfig> {
    if !path.exists() {
        return None;
    }

    let contents = fs::read_to_string(&path).ok()?;
    serde_yaml::from_str(&contents).ok()
}

fn parse_non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// 生成配置文件的候选搜索路径列表
///
/// 按优先级搜索：
/// 1. 绝对路径
/// 2. AITRADER_REPO_ROOT 环境变量指定的目录
/// 3. CARGO_MANIFEST_DIR 及其父目录
/// 4. 当前工作目录及其父目录
/// 5. 相对路径 ../ 和 ../../
fn candidate_paths(config_path: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let target = PathBuf::from(config_path);

    // 如果是绝对路径，直接使用
    if target.is_absolute() {
        candidates.push(target);
        return candidates;
    }

    // 从 AITRADER_REPO_ROOT 环境变量搜索
    if let Ok(repo_root) = env::var("AITRADER_REPO_ROOT") {
        let base = PathBuf::from(repo_root);
        push_candidate(&base.join(config_path), &mut candidates, &mut seen);
    }

    // 从 Cargo manifest 目录及其父目录搜索
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

    // 从当前工作目录及其父目录搜索
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

/// 添加候选路径（避免重复）
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

/// 连接 PostgreSQL 数据库
///
/// 建立异步数据库连接，并在后台任务中维护连接
async fn connect_client(url: &str) -> Result<Client> {
    let (client, connection) = tokio_postgres::connect(url, NoTls).await?;
    // 在后台维护数据库连接
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            warn!(%err, "postgres connection error");
        }
    });
    Ok(client)
}

/// 创建数据库表（如果不存在）
///
/// 在数据库初始化时执行所有 CREATE TABLE IF NOT EXISTS 语句
/// - 如果表已存在，则跳过（不会修改现有表结构）
/// - 如果表不存在，则创建新表
async fn create_tables_if_not_exists(client: &Client, schema: &str) -> Result<()> {
    for statement in table_creation_statements(schema) {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }

        match client.batch_execute(trimmed).await {
            Ok(_) => {}
            // pgcrypto 扩展失败时只警告，不中断（可能权限不足）
            Err(err) if trimmed.starts_with("CREATE EXTENSION IF NOT EXISTS pgcrypto") => {
                if let Some(db_err) = err.as_db_error() {
                    let code = db_err.code().code();
                    warn!(
                        code = code,
                        message = db_err.message(),
                        detail = db_err.detail().unwrap_or_default(),
                        hint = db_err.hint().unwrap_or_default(),
                        "创建 pgcrypto 扩展失败，继续执行（可能是权限不足）"
                    );
                } else {
                    warn!(?err, "创建 pgcrypto 扩展失败，继续执行");
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
                        "创建表失败"
                    );
                } else {
                    warn!(?err, stmt = trimmed, "创建表失败");
                }
                return Err(err.into());
            }
        }
    }

    Ok(())
}

/// 生成数据库表创建 SQL 语句列表
///
/// **返回的 SQL 语句（所有都是 CREATE IF NOT EXISTS，不会修改已有表）：**
///
/// 1. **CREATE EXTENSION pgcrypto**: 启用 UUID 生成功能（gen_random_uuid）
///
/// 2. **CREATE SCHEMA**: 创建独立的 schema（类似命名空间），避免与其他应用冲突
///
/// 3. **strategies 表**: 存储 AI 策略分析结果
///    - id: 唯一标识
///    - summary: 策略分析摘要
///    - created_at: 创建时间
///
/// 4. **orders 表**: 存储订单记录
///    - symbol: 交易对（如 BTC-USDT-SWAP）
///    - side: 买卖方向（buy/sell）
///    - price/size: 价格和数量
///    - status: 订单状态
///    - metadata: 额外信息（JSON 格式）
///
/// 5. **balances 表**: 存储账户余额快照（用于绘制权益曲线）
///    - asset: 资产类型（如 USDT）
///    - available: 可用余额
///    - locked: 冻结余额
///    - valuation: 总估值
///
/// 6. **initial_equities 表**: 存储初始资金记录
///    - amount: 初始资金金额
///    - recorded_at: 记录时间
fn table_creation_statements(schema: &str) -> Vec<String> {
    vec![
        // 1. 创建 pgcrypto 扩展（用于生成 UUID）
        "CREATE EXTENSION IF NOT EXISTS pgcrypto;".to_string(),
        // 2. 创建 schema（数据库命名空间）
        format!("CREATE SCHEMA IF NOT EXISTS {schema};", schema = schema),
        // 3. 创建策略表（存储 AI 分析结果）
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.strategies (
                id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                summary         TEXT NOT NULL,
                created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
            );",
            schema = schema,
        ),
        // 4. 创建订单表（存储交易订单）
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
                metadata        JSONB NOT NULL DEFAULT '{{}}'::jsonb,
                created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
                closed_at       TIMESTAMPTZ
            );",
            schema = schema,
        ),
        // 5. 创建余额快照表（用于绘制权益曲线）
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.balances (
                id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                asset           TEXT NOT NULL DEFAULT 'USDT',
                available       NUMERIC(20, 8) NOT NULL,
                locked          NUMERIC(20, 8) NOT NULL,
                valuation       NUMERIC(20, 8) NOT NULL,
                source          TEXT NOT NULL DEFAULT 'okx',
                recorded_at     TIMESTAMPTZ NOT NULL DEFAULT now()
            );",
            schema = schema,
        ),
        // 6. 创建初始资金表
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

/// 初始化数据库
///
/// 应用启动时调用此函数，执行以下操作：
/// 1. 从配置文件读取数据库连接信息
/// 2. 连接到 PostgreSQL 数据库
/// 3. **如果 RESET_DATABASE=true，则删除整个 schema 并重建（危险操作！）**
/// 4. 创建所有必需的表（如果不存在）
///
/// **环境变量：**
/// - `RESET_DATABASE=true`: 启动时清空并重建数据库（会删除所有数据！）
///
/// 如果数据库未配置或连接失败，会记录警告但不中断应用启动
pub async fn init_database(should_reset: bool) -> Result<()> {
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

    if should_reset {
        warn!(
            schema = %schema,
            "⚠️  RESET_DATABASE=true 检测到，将删除并重建 schema（所有数据将丢失）"
        );

        // 删除整个 schema（CASCADE 会删除所有表）
        let drop_sql = format!("DROP SCHEMA IF EXISTS {schema} CASCADE;", schema = schema);
        match client.batch_execute(&drop_sql).await {
            Ok(_) => info!(schema = %schema, "Schema 已删除"),
            Err(err) => {
                warn!(%err, schema = %schema, "删除 schema 失败");
                return Err(err.into());
            }
        }
    }

    create_tables_if_not_exists(&client, schema.as_str()).await?;
    info!("数据库初始化完成");

    Ok(())
}

/// 策略消息插入载荷（用于写入数据库）
#[derive(Debug, Clone)]
pub struct StrategyMessageInsert {
    /// 策略分析摘要内容
    pub summary: String,
}

/// 策略消息记录（从数据库读取）
#[derive(Debug, Clone)]
pub struct StrategyMessageRecord {
    /// 记录唯一标识
    pub id: Uuid,
    /// 策略分析摘要内容
    pub summary: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

/// 插入策略消息到数据库
///
/// 将 AI 生成的策略分析结果存储到 strategies 表
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
        "INSERT INTO {schema}.strategies (summary)
         VALUES ($1);",
        schema = schema,
    );

    client
        .execute(&sql, &[&payload.summary])
        .await
        .map(|_| ())
        .map_err(|err| {
            warn!(%err, "插入 strategy 记录失败");
            err.into()
        })
}

/// 查询最近的策略消息列表
///
/// 按创建时间倒序返回指定数量的策略记录，用于前端展示历史分析
pub async fn fetch_strategy_messages(limit: i64) -> Result<Vec<StrategyMessageRecord>> {
    let DatabaseSettings { url, schema } = database_settings();

    let Some(url) = url else {
        warn!("未配置数据库连接字符串，跳过策略对话查询");
        return Ok(Vec::new());
    };

    let client = connect_client(&url).await?;
    let sql = format!(
        "SELECT id::text AS id_text, summary, created_at
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
            created_at: row.get("created_at"),
        });
    }

    Ok(records)
}

#[allow(dead_code)]
pub async fn fetch_initial_equity() -> Result<Option<(f64, DateTime<Utc>)>> {
    let DatabaseSettings { url, schema } = database_settings();

    let url = match url {
        Some(url) => url,
        None => {
            warn!("无法读取数据库配置，跳过初始资金查询");
            return Ok(None);
        }
    };
    info!(
        %schema,
        "fetch_initial_equity 123 connecting to database for initial_equities query"
    );
    let client = connect_client(&url).await?;
    let sql = format!(
        "SELECT amount::double precision AS amount, recorded_at \
         FROM {schema}.initial_equities \
         ORDER BY recorded_at DESC \
         LIMIT 1;",
        schema = schema
    );
    info!(%schema, "fetch_initial_equity executing query: {sql}");
    match tokio::time::timeout(Duration::from_secs(5), client.query_opt(&sql, &[])).await {
        Ok(Ok(Some(row))) => {
            let amount: f64 = row.try_get("amount")?;
            let recorded_at: DateTime<Utc> = row.get("recorded_at");
            info!(
                %schema,
                amount,
                recorded_at = %recorded_at,
                "fetch_initial_equity found record"
            );
            Ok(Some((amount, recorded_at)))
        }
        Ok(Ok(None)) => {
            info!(%schema, "fetch_initial_equity found no record");
            Ok(None)
        }
        Ok(Err(err)) => {
            warn!(
                %schema,
                error = ?err,
                "fetch_initial_equity query execution failed"
            );
            Err(err.into())
        }
        Err(_) => {
            warn!(%schema, "fetch_initial_equity query timed out");
            Err(anyhow!("initial_equity_query_timeout"))
        }
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
        "INSERT INTO {schema}.initial_equities (amount) VALUES (($1::double precision)::numeric(20, 8));",
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

#[derive(Debug, Clone)]
pub struct BalanceSnapshotRecord {
    pub asset: String,
    pub available: f64,
    pub locked: f64,
    pub valuation: f64,
    pub source: String,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct BalanceSnapshotInsert {
    pub asset: String,
    pub available: f64,
    pub locked: f64,
    pub valuation: f64,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct AgentOrderEvent {
    pub ord_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: Option<String>,
    pub price: Option<f64>,
    pub size: f64,
    pub filled_size: Option<f64>,
    pub status: String,
    pub metadata: Value,
}

pub async fn upsert_agent_order(event: AgentOrderEvent) -> Result<()> {
    let DatabaseSettings { url, schema } = database_settings();

    let url = match url {
        Some(url) => url,
        None => {
            warn!("未配置数据库连接字符串，无法写入 agent order");
            return Err(anyhow!("missing database url"));
        }
    };

    let client = connect_client(&url).await?;
    let metadata = normalize_order_metadata(event.metadata, &event.ord_id);
    let status = event.status.clone();
    let is_terminal = is_terminal_status(&status);

    let update_sql = format!(
        "UPDATE {schema}.orders
         SET status = $2,
             filled_size = COALESCE($3, filled_size),
             metadata = metadata || $4,
             closed_at = CASE WHEN $5 THEN NOW() ELSE closed_at END
         WHERE metadata->>'ordId' = $1;",
        schema = schema,
    );

    let rows_updated = client
        .execute(
            &update_sql,
            &[
                &event.ord_id,
                &status,
                &event.filled_size,
                &metadata,
                &is_terminal,
            ],
        )
        .await?;

    if rows_updated == 0 {
        let insert_sql = format!(
            "INSERT INTO {schema}.orders (
                symbol,
                side,
                order_type,
                price,
                size,
                filled_size,
                status,
                metadata
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8);",
            schema = schema
        );
        client
            .execute(
                &insert_sql,
                &[
                    &event.symbol,
                    &event.side,
                    &event.order_type,
                    &event.price,
                    &event.size,
                    &event.filled_size,
                    &status,
                    &metadata,
                ],
            )
            .await?;
    }

    Ok(())
}

fn normalize_order_metadata(metadata: Value, ord_id: &str) -> Value {
    match metadata {
        Value::Object(mut map) => {
            map.entry("ordId")
                .or_insert_with(|| Value::String(ord_id.to_string()));
            Value::Object(map)
        }
        other => {
            let mut map = Map::new();
            map.insert("ordId".to_string(), Value::String(ord_id.to_string()));
            map.insert("payload".to_string(), other);
            Value::Object(map)
        }
    }
}

fn is_terminal_status(status: &str) -> bool {
    let normalized = status.to_lowercase();
    normalized.contains("filled")
        || normalized.contains("cancel")
        || normalized.contains("closed")
        || normalized.contains("reject")
}

pub async fn fetch_latest_balance_snapshot(asset: &str) -> Result<Option<BalanceSnapshotRecord>> {
    let DatabaseSettings { url, schema } = database_settings();

    let url = match url {
        Some(url) => url,
        None => {
            warn!("未配置数据库连接字符串，无法查询余额快照");
            return Ok(None);
        }
    };

    let client = connect_client(&url).await?;
    let sql = format!(
        "SELECT asset,
                available::double precision AS available,
                locked::double precision AS locked,
                valuation::double precision AS valuation,
                source,
                recorded_at
         FROM {schema}.balances
         WHERE asset = $1
         ORDER BY recorded_at DESC
         LIMIT 1;",
        schema = schema
    );
    if let Some(row) = client.query_opt(&sql, &[&asset]).await? {
        Ok(Some(BalanceSnapshotRecord {
            asset: row.get("asset"),
            available: row.get("available"),
            locked: row.get("locked"),
            valuation: row.get("valuation"),
            source: row.get("source"),
            recorded_at: row.get("recorded_at"),
        }))
    } else {
        Ok(None)
    }
}

pub async fn fetch_balance_snapshots(
    asset: &str,
    limit: i64,
) -> Result<Vec<BalanceSnapshotRecord>> {
    let DatabaseSettings { url, schema } = database_settings();

    let url = match url {
        Some(url) => url,
        None => {
            warn!("未配置数据库连接字符串，无法查询余额快照");
            return Ok(Vec::new());
        }
    };

    let client = connect_client(&url).await?;
    let sql = format!(
        "SELECT asset,
                available::double precision AS available,
                locked::double precision AS locked,
                valuation::double precision AS valuation,
                source,
                recorded_at
         FROM {schema}.balances
         WHERE asset = $1
         ORDER BY recorded_at DESC
         LIMIT $2;",
        schema = schema
    );
    let rows = client.query(&sql, &[&asset, &limit]).await?;

    let mut records = Vec::with_capacity(rows.len());
    for row in rows {
        records.push(BalanceSnapshotRecord {
            asset: row.get("asset"),
            available: row.get("available"),
            locked: row.get("locked"),
            valuation: row.get("valuation"),
            source: row.get("source"),
            recorded_at: row.get("recorded_at"),
        });
    }

    Ok(records)
}

pub async fn insert_balance_snapshot(snapshot: BalanceSnapshotInsert) -> Result<()> {
    let DatabaseSettings { url, schema } = database_settings();

    let url = match url {
        Some(url) => url,
        None => {
            warn!("未配置数据库连接字符串，无法写入余额快照");
            return Err(anyhow!("missing database url"));
        }
    };

    let client = connect_client(&url).await?;
    let sql = format!(
        "INSERT INTO {schema}.balances (asset, available, locked, valuation, source)
         VALUES (
             $1,
             ($2::double precision)::numeric(20, 8),
             ($3::double precision)::numeric(20, 8),
             ($4::double precision)::numeric(20, 8),
             $5
         );",
        schema = schema
    );
    client
        .execute(
            &sql,
            &[
                &snapshot.asset,
                &snapshot.available,
                &snapshot.locked,
                &snapshot.valuation,
                &snapshot.source,
            ],
        )
        .await?;

    Ok(())
}
