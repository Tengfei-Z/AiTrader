use std::net::SocketAddr;

use ::config::{Config, ConfigError as BuilderError, Environment, File};
use serde::Deserialize;
use thiserror::Error;

const DEFAULT_CONFIG_PATH: &str = "config/config.yaml";

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct BackendConfig {
    #[serde(default = "default_bind")]
    pub bind_addr: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct DeploymentConfig {
    #[serde(default)]
    pub backend: Option<BackendConfig>,
    #[serde(default)]
    pub runtime_env: Option<RuntimeEnvConfig>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub server: Option<ServerConfig>,
    #[serde(default)]
    pub deployment: Option<DeploymentConfig>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct RuntimeEnvConfig {
    #[serde(default)]
    pub http_proxy: Option<String>,
    #[serde(default)]
    pub https_proxy: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid socket address: {0}")]
    InvalidAddr(String),
    #[error("configuration load failed: {0}")]
    Load(#[from] BuilderError),
}

fn default_bind() -> String {
    "0.0.0.0:3000".to_string()
}

impl AppConfig {
    pub fn bind_addr(&self) -> Result<SocketAddr, ConfigError> {
        if let Some(server) = &self.server {
            return server
                .bind
                .parse()
                .map_err(|_| ConfigError::InvalidAddr(server.bind.clone()));
        }

        if let Some(deployment) = &self.deployment {
            if let Some(backend) = &deployment.backend {
                return backend
                    .bind_addr
                    .parse()
                    .map_err(|_| ConfigError::InvalidAddr(backend.bind_addr.clone()));
            }
        }

        let fallback = default_bind();
        fallback
            .parse()
            .map_err(|_| ConfigError::InvalidAddr(fallback))
    }

    pub fn apply_runtime_env(&self) {
        if let Some(deployment) = &self.deployment {
            if let Some(runtime) = &deployment.runtime_env {
                if let Some(proxy) = &runtime.http_proxy {
                    std::env::set_var("HTTP_PROXY", proxy);
                    std::env::set_var("http_proxy", proxy);
                }
                if let Some(proxy) = &runtime.https_proxy {
                    std::env::set_var("HTTPS_PROXY", proxy);
                    std::env::set_var("https_proxy", proxy);
                }
            }
        }
    }
}

pub fn load_app_config() -> Result<AppConfig, ConfigError> {
    let mut builder = Config::builder();

    builder = builder.add_source(File::with_name(DEFAULT_CONFIG_PATH).required(false));

    builder = builder.add_source(Environment::with_prefix("AITRADER").separator("__"));

    let config: AppConfig = builder.build()?.try_deserialize()?;

    Ok(config)
}
