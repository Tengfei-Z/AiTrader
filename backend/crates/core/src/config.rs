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
pub struct DeepSeekConfig {
    pub api_key: String,
    pub endpoint: String,
    #[serde(default = "default_model")]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_okx_rest_endpoint")]
    pub okx_rest_endpoint: String,
    pub okx_credentials: Option<OkxCredentials>,
    pub deepseek: Option<DeepSeekConfig>,
    pub mcp: Option<McpConfig>,
}

impl AppConfig {
    /// Build configuration from well-known environment variables.
    pub fn load_from_env() -> Result<Self> {
        preload_env_files();

        let okx_credentials = match (
            env_var_non_empty("OKX_API_KEY"),
            env_var_non_empty("OKX_API_SECRET"),
            env_var_non_empty("OKX_PASSPHRASE"),
        ) {
            (Ok(api_key), Ok(api_secret), Ok(passphrase)) => Some(OkxCredentials {
                api_key,
                api_secret,
                passphrase,
            }),
            _ => None,
        };

        let deepseek = match (
            env_var_non_empty("DEEPSEEK_API_KEY"),
            env_var_non_empty("DEEPSEEK_ENDPOINT"),
        ) {
            (Ok(api_key), Ok(endpoint)) => {
                let model = env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| default_model());
                Some(DeepSeekConfig {
                    api_key,
                    endpoint,
                    model,
                })
            }
            _ => None,
        };

        let mcp = match env_var_non_empty("MCP_EXECUTABLE") {
            Ok(executable) => {
                let args = env::var("MCP_ARGS")
                    .ok()
                    .and_then(|value| {
                        if value.trim().is_empty() {
                            None
                        } else {
                            Some(
                                value
                                    .split_whitespace()
                                    .map(|s| s.to_string())
                                    .collect::<Vec<_>>(),
                            )
                        }
                    })
                    .unwrap_or_default();
                Some(McpConfig { executable, args })
            }
            Err(_) => None,
        };

        let okx_rest_endpoint =
            env::var("OKX_REST_ENDPOINT").unwrap_or_else(|_| default_okx_rest_endpoint());

        Ok(Self {
            okx_rest_endpoint,
            okx_credentials,
            deepseek,
            mcp,
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

    /// Helper that forces the presence of DeepSeek configuration.
    pub fn require_deepseek_config(&self) -> Result<&DeepSeekConfig> {
        let config = self
            .deepseek
            .as_ref()
            .context(
                "未找到 DeepSeek 配置：请在当前目录创建 .env（可参考 .env.example），并设置 DEEPSEEK_API_KEY、DEEPSEEK_ENDPOINT",
            )?;

        ensure!(
            !config.api_key.trim().is_empty() && !config.endpoint.trim().is_empty(),
            "DeepSeek 配置不能为空：请在 .env 中填写 DEEPSEEK_API_KEY 与 DEEPSEEK_ENDPOINT"
        );

        Ok(config)
    }

    /// Helper that forces the presence of MCP 配置.
    pub fn require_mcp_config(&self) -> Result<&McpConfig> {
        let config = self.mcp.as_ref().context(
            "未找到 MCP 配置：请在当前目录创建 .env（可参考 .env.example），并设置 MCP_EXECUTABLE",
        )?;

        ensure!(
            !config.executable.trim().is_empty(),
            "MCP_EXECUTABLE 不能为空，请在 .env 中填写可执行文件路径"
        );

        Ok(config)
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

fn default_model() -> String {
    "deepseek-chat".to_string()
}

fn preload_env_files() {
    // 自动加载当前目录或上层目录中的 .env 文件（如果存在）
    let _ = dotenv();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate_files = [
        manifest_dir.join("../../.env"),
        manifest_dir.join("../okx/.env"),
    ];

    for path in candidate_files {
        if path.exists() {
            let _ = dotenvy::from_path(path);
        }
    }
}
