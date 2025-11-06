use anyhow::{ensure, Context, Result};
use dotenvy::dotenv;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;

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
    #[serde(default = "default_okx_rest_endpoint")]
    pub okx_rest_endpoint: String,
    pub okx_credentials: Option<OkxCredentials>,
    pub okx_simulated_credentials: Option<OkxCredentials>,
    pub agent: Option<AgentConfig>,
}

impl AppConfig {
    /// Build configuration from well-known environment variables.
    pub fn load_from_env() -> Result<Self> {
        preload_env_files();

        let okx_credentials =
            load_okx_credentials("OKX_API_KEY", "OKX_API_SECRET", "OKX_PASSPHRASE");
        let okx_simulated_credentials = load_okx_credentials(
            "OKX_SIM_API_KEY",
            "OKX_SIM_API_SECRET",
            "OKX_SIM_PASSPHRASE",
        );

        let okx_rest_endpoint =
            env::var("OKX_REST_ENDPOINT").unwrap_or_else(|_| default_okx_rest_endpoint());

        let agent = match env_var_non_empty("AGENT_BASE_URL") {
            Ok(base_url) => Some(AgentConfig { base_url }),
            Err(_) => None,
        };

        Ok(Self {
            okx_rest_endpoint,
            okx_credentials,
            okx_simulated_credentials,
            agent,
        })
    }

    /// Helper that forces the presence of OKX credentials.
    pub fn require_okx_credentials(&self) -> Result<&OkxCredentials> {
        let credentials = self
            .okx_credentials
            .as_ref()
            .context(
                "未找到 OKX 凭证：请在当前目录创建 .env（可参考 .env.example），并设置 OKX_API_KEY、OKX_API_SECRET、OKX_PASSPHRASE",
            )?;

        ensure!(
            !credentials.api_key.trim().is_empty()
                && !credentials.api_secret.trim().is_empty()
                && !credentials.passphrase.trim().is_empty(),
            "OKX 凭证不能为空：请在 .env 中填写 OKX_API_KEY、OKX_API_SECRET、OKX_PASSPHRASE"
        );

        Ok(credentials)
    }

    pub fn require_okx_simulated_credentials(&self) -> Result<&OkxCredentials> {
        let credentials = self
            .okx_simulated_credentials
            .as_ref()
            .context(
                "未找到 OKX 模拟账户凭证：请在当前目录创建 .env，并设置 OKX_SIM_API_KEY、OKX_SIM_API_SECRET、OKX_SIM_PASSPHRASE",
            )?;

        ensure!(
            !credentials.api_key.trim().is_empty()
                && !credentials.api_secret.trim().is_empty()
                && !credentials.passphrase.trim().is_empty(),
            "OKX 模拟账户凭证不能为空：请在 .env 中填写 OKX_SIM_API_KEY、OKX_SIM_API_SECRET、OKX_SIM_PASSPHRASE"
        );

        Ok(credentials)
    }

    pub fn agent_base_url(&self) -> Option<&str> {
        self.agent.as_ref().map(|cfg| cfg.base_url.as_str())
    }
}

fn env_var_non_empty(key: &str) -> Result<String, env::VarError> {
    let value = env::var(key)?;
    if value.trim().is_empty() {
        return Err(env::VarError::NotPresent);
    }
    Ok(value)
}

fn default_okx_rest_endpoint() -> String {
    "https://www.okx.com".to_string()
}

fn preload_env_files() {
    // 自动加载当前目录或上层目录中的 .env 文件（如果存在）
    let _ = dotenv();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate_files = [
        manifest_dir.join(".env"),
        manifest_dir.join("../.env"),
    ];

    for path in candidate_files {
        if path.exists() {
            let _ = dotenvy::from_path(path);
        }
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
