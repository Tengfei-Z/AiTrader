use crate::error::OkxError;
use ai_core::config::{AppConfig, OkxCredentials};
use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use reqwest::{Client, Method, RequestBuilder};
use serde::de::DeserializeOwned;
use serde_json::{self, Value};
use tracing::instrument;

const API_PREFIX: &str = "/api/v5";

#[derive(Debug, Clone)]
pub struct OkxRestClient {
    http: Client,
    base_url: String,
    credentials: OkxCredentials,
}

#[derive(Debug, Clone, Default)]
pub struct ProxyOptions {
    pub http: Option<String>,
    pub https: Option<String>,
}

impl OkxRestClient {
    pub fn from_config(config: &AppConfig) -> Result<Self> {
        let credentials = config.require_okx_credentials()?.clone();
        Self::new_with_proxy(
            config.okx_rest_endpoint.clone(),
            credentials,
            ProxyOptions::default(),
        )
    }

    pub fn from_config_with_proxy(config: &AppConfig, proxy: ProxyOptions) -> Result<Self> {
        let credentials = config.require_okx_credentials()?.clone();
        Self::new_with_proxy(config.okx_rest_endpoint.clone(), credentials, proxy)
    }

    pub fn new(base_url: impl Into<String>, credentials: OkxCredentials) -> Result<Self> {
        Self::new_with_proxy(base_url, credentials, ProxyOptions::default())
    }

    pub fn new_with_proxy(
        base_url: impl Into<String>,
        credentials: OkxCredentials,
        proxy: ProxyOptions,
    ) -> Result<Self> {
        let mut builder = Client::builder()
            .user_agent("ai-trader-backend/0.1")
            .danger_accept_invalid_certs(true);

        if let Some(ref http_proxy) = proxy.http {
            tracing::info!("configuring HTTP proxy {}", http_proxy);
            let http = reqwest::Proxy::http(http_proxy)?;
            builder = builder.proxy(http);
        }

        if let Some(ref https_proxy) = proxy.https {
            tracing::info!("configuring HTTPS proxy {}", https_proxy);
            let https = reqwest::Proxy::https(https_proxy)?;
            builder = builder.proxy(https);
        }

        let http = builder.build().map_err(OkxError::from)?;

        Ok(Self {
            http,
            base_url: base_url.into(),
            credentials,
        })
    }

    #[instrument(skip(self))]
    pub async fn get_server_time(&self) -> Result<u64> {
        #[derive(serde::Deserialize)]
        struct TimeResponse {
            data: Vec<TimeData>,
        }

        #[derive(serde::Deserialize)]
        struct TimeData {
            ts: String,
        }

        let url = format!("{API_PREFIX}/public/time");
        let response: TimeResponse = self.get(&url, None).await?;
        let timestamp = response
            .data
            .first()
            .ok_or_else(|| OkxError::EmptyResponse("public/time".into()))?;

        timestamp
            .ts
            .parse::<u64>()
            .map_err(|err| OkxError::Deserialize(err.into()).into())
    }

    #[instrument(skip(self), fields(inst_id = %inst_id))]
    pub async fn get_ticker(&self, inst_id: &str) -> Result<crate::models::Ticker> {
        #[derive(serde::Deserialize)]
        struct TickerResponse {
            data: Vec<crate::models::Ticker>,
        }

        let path = format!("{API_PREFIX}/market/ticker?instId={}", inst_id);
        let response: TickerResponse = self.get(&path, None).await?;
        response
            .data
            .into_iter()
            .next()
            .ok_or_else(|| OkxError::EmptyResponse("market/ticker".into()).into())
    }

    #[instrument(skip(self))]
    pub async fn get_account_balance(&self) -> Result<crate::models::AccountBalanceResponse> {
        let path = format!("{API_PREFIX}/account/balance");
        let response: crate::models::AccountBalanceResponse = self.get(&path, None).await?;
        Ok(response)
    }

    #[instrument(skip(self), fields(inst_type = inst_type.unwrap_or("all")))]
    pub async fn get_positions(
        &self,
        inst_type: Option<&str>,
    ) -> Result<Vec<crate::models::PositionDetail>> {
        #[derive(serde::Deserialize)]
        struct ResponseWrapper {
            data: Vec<crate::models::PositionDetail>,
        }

        let mut path = format!("{API_PREFIX}/account/positions");
        if let Some(inst_type) = inst_type {
            path.push_str(&format!("?instType={}", inst_type));
        }

        let response: ResponseWrapper = self.get(&path, None).await?;
        Ok(response.data)
    }

    async fn get<T>(&self, path_and_query: &str, body: Option<Value>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        tracing::info!("OKX GET {}", path_and_query);
        let builder = self.prepare_request(Method::GET, path_and_query, body)?;
        self.execute(builder).await
    }

    fn prepare_request(
        &self,
        method: Method,
        path_and_query: &str,
        body: Option<Value>,
    ) -> Result<RequestBuilder> {
        let url = format!("{}{}", self.base_url, path_and_query);
        let timestamp = current_timestamp_iso();
        let payload_json = body
            .as_ref()
            .map(|payload| serde_json::to_string(payload))
            .transpose()
            .map_err(OkxError::Serialize)?;

        let sign = crate::models::sign_request(
            &timestamp,
            method.as_str(),
            path_and_query,
            payload_json.as_deref(),
            &self.credentials.api_secret,
        )?;

        let mut headers = HeaderMap::new();
        headers.insert(
            "OK-ACCESS-KEY",
            HeaderValue::from_str(&self.credentials.api_key)?,
        );
        headers.insert(
            "OK-ACCESS-PASSPHRASE",
            HeaderValue::from_str(&self.credentials.passphrase)?,
        );
        headers.insert("OK-ACCESS-TIMESTAMP", HeaderValue::from_str(&timestamp)?);
        headers.insert("OK-ACCESS-SIGN", HeaderValue::from_str(&sign)?);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let builder = self.http.request(method, url).headers(headers);
        Ok(match payload_json {
            Some(payload) => builder.body(payload),
            None => builder,
        })
    }

    async fn execute<T>(&self, builder: RequestBuilder) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let response = builder.send().await.map_err(OkxError::from)?;
        let status = response.status();

        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read body>".to_string());
            return Err(OkxError::HttpStatusWithBody { status, body }.into());
        }

        response
            .json::<T>()
            .await
            .map_err(|err| OkxError::Deserialize(err.into()).into())
    }
}

fn current_timestamp_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
