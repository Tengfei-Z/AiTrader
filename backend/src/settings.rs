use anyhow::{ensure, Context, Result};
use dotenvy::dotenv;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use tracing::info;

/// Global configuration accessor to keep the rest of the application stateless.
pub static CONFIG: Lazy<AppConfig> = Lazy::new(|| {
    AppConfig::load_from_env().expect("failed to load configuration from environment")
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxCredentials {
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_okx_base_url")]
    pub okx_base_url: String,
    pub okx_credentials: Option<OkxCredentials>,
    pub agent: Option<AgentConfig>,
    #[serde(default = "default_okx_use_simulated")]
    pub okx_use_simulated: bool,
    #[serde(default = "default_initial_equity")]
    pub initial_equity: f64,
    #[serde(default = "default_reset_database")]
    pub reset_database: bool,
    #[serde(default = "default_strategy_schedule_enabled")]
    pub strategy_schedule_enabled: bool,
    #[serde(default = "default_strategy_schedule_interval_secs")]
    pub strategy_schedule_interval_secs: u64,
}

impl AppConfig {
    /// Build configuration from well-known environment variables.
    pub fn load_from_env() -> Result<Self> {
        preload_env_files();

        let okx_credentials =
            load_okx_credentials("OKX_API_KEY", "OKX_SECRET_KEY", "OKX_PASSPHRASE");

        let okx_base_url = env::var("OKX_BASE_URL").unwrap_or_else(|_| default_okx_base_url());
        let okx_use_simulated = env_bool("OKX_USE_SIMULATED", true);

        let agent = match env_var_non_empty("AGENT_BASE_URL") {
            Ok(base_url) => Some(AgentConfig { base_url }),
            Err(_) => None,
        };

        let initial_equity = env::var("INITIAL_EQUITY")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or_else(default_initial_equity);

        let reset_database = env_bool("RESET_DATABASE", false);

        let strategy_schedule_enabled = env_bool("STRATEGY_SCHEDULE_ENABLED", false);
        let strategy_schedule_interval_secs = env::var("STRATEGY_SCHEDULE_INTERVAL_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|secs| *secs > 0)
            .unwrap_or_else(default_strategy_schedule_interval_secs);

        Ok(Self {
            okx_base_url,
            okx_credentials,
            agent,
            okx_use_simulated,
            initial_equity,
            reset_database,
            strategy_schedule_enabled,
            strategy_schedule_interval_secs,
        })
    }

    pub fn require_okx_credentials(&self) -> Result<&OkxCredentials> {
        let credentials = self.okx_credentials.as_ref().context(
            "未找到 OKX 凭证：请在 .env 中设置 OKX_API_KEY、OKX_SECRET_KEY、OKX_PASSPHRASE",
        )?;

        ensure!(
            !credentials.api_key.trim().is_empty()
                && !credentials.api_secret.trim().is_empty()
                && !credentials.passphrase.trim().is_empty(),
            "OKX 凭证不能为空：请在 .env 中填写 OKX_API_KEY、OKX_SECRET_KEY、OKX_PASSPHRASE"
        );

        Ok(credentials)
    }

    pub fn agent_base_url(&self) -> Option<&str> {
        self.agent.as_ref().map(|cfg| cfg.base_url.as_str())
    }

    pub fn okx_use_simulated(&self) -> bool {
        self.okx_use_simulated
    }

    pub fn should_reset_database(&self) -> bool {
        self.reset_database
    }

    pub fn strategy_schedule_enabled(&self) -> bool {
        self.strategy_schedule_enabled
    }

    pub fn strategy_schedule_interval_secs(&self) -> u64 {
        self.strategy_schedule_interval_secs
    }
}

fn env_var_non_empty(key: &str) -> Result<String, env::VarError> {
    let value = env::var(key)?;
    if value.trim().is_empty() {
        return Err(env::VarError::NotPresent);
    }
    Ok(value)
}

fn default_okx_base_url() -> String {
    "https://www.okx.com".to_string()
}

fn default_okx_use_simulated() -> bool {
    true
}

fn default_initial_equity() -> f64 {
    122_000.0
}

fn default_reset_database() -> bool {
    false
}

fn default_strategy_schedule_enabled() -> bool {
    false
}

fn default_strategy_schedule_interval_secs() -> u64 {
    300
}

fn env_bool(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on" | "enabled")
        }
        Err(_) => default,
    }
}

fn preload_env_files() {
    // 自动加载当前目录或上层目录中的 .env 文件（如果存在）
    let mut loaded_paths = Vec::new();
    if dotenv().is_ok() {
        loaded_paths.push(".env".to_string());
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate_files = [manifest_dir.join(".env"), manifest_dir.join("../.env")];

    for path in candidate_files {
        if path.exists() && dotenvy::from_path(&path).is_ok() {
            loaded_paths.push(path.display().to_string());
        }
    }

    if loaded_paths.is_empty() {
        info!(
            message = "falling back to process environment only",
            "env_files_not_found"
        );
    } else {
        info!(paths = %loaded_paths.join(", "), "env_files_loaded");
    }
}

fn load_okx_credentials(key: &str, secret: &str, passphrase: &str) -> Option<OkxCredentials> {
    match (
        env_var_non_empty(key),
        env_var_non_empty(secret),
        env_var_non_empty(passphrase),
    ) {
        (Ok(api_key), Ok(api_secret), Ok(passphrase)) => Some(OkxCredentials {
            api_key,
            api_secret,
            passphrase,
        }),
        _ => None,
    }
}
