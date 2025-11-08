use super::error::OkxError;
use crate::settings::{AppConfig, OkxCredentials};
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
    simulated_trading: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ProxyOptions {
    pub http: Option<String>,
    pub https: Option<String>,
}

impl OkxRestClient {
    pub fn from_config_with_proxy(
        config: &AppConfig,
        proxy: ProxyOptions,
        simulated_trading: bool,
    ) -> Result<Self> {
        let credentials = config.require_okx_credentials(simulated_trading)?.clone();
        Self::new_with_proxy(
            config.okx_base_url.clone(),
            credentials,
            proxy,
            simulated_trading,
        )
    }

    fn new_with_proxy(
        base_url: impl Into<String>,
        credentials: OkxCredentials,
        proxy: ProxyOptions,
        simulated_trading: bool,
    ) -> Result<Self> {
        let mut builder = Client::builder()
            .user_agent("ai-trader-backend/0.1")
            .danger_accept_invalid_certs(true);

        if let Some(ref http_proxy) = proxy.http {
            tracing::debug!("configuring HTTP proxy {}", http_proxy);
            let http = reqwest::Proxy::http(http_proxy)?;
            builder = builder.proxy(http);
        }

        if let Some(ref https_proxy) = proxy.https {
            tracing::debug!("configuring HTTPS proxy {}", https_proxy);
            let https = reqwest::Proxy::https(https_proxy)?;
            builder = builder.proxy(https);
        }

        let http = builder.build().map_err(OkxError::from)?;

        Ok(Self {
            http,
            base_url: base_url.into(),
            credentials,
            simulated_trading,
        })
    }

    #[instrument(skip(self), fields(inst_id = %inst_id))]
    pub async fn get_ticker(&self, inst_id: &str) -> Result<super::models::Ticker> {
        #[derive(serde::Deserialize)]
        struct TickerResponse {
            data: Vec<super::models::Ticker>,
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
    pub async fn get_account_balance(&self) -> Result<super::models::AccountBalanceResponse> {
        let path = format!("{API_PREFIX}/account/balance");
        let response: super::models::AccountBalanceResponse = self.get(&path, None).await?;
        Ok(response)
    }

    #[instrument(skip(self), fields(inst_type = inst_type.unwrap_or("all")))]
    pub async fn get_positions(
        &self,
        inst_type: Option<&str>,
    ) -> Result<Vec<super::models::PositionDetail>> {
        #[derive(serde::Deserialize)]
        struct ResponseWrapper {
            data: Vec<super::models::PositionDetail>,
        }

        let mut path = format!("{API_PREFIX}/account/positions");
        if let Some(inst_type) = inst_type {
            path.push_str(&format!("?instType={}", inst_type));
        }

        let response: ResponseWrapper = self.get(&path, None).await?;
        Ok(response.data)
    }

    #[instrument(skip(self), fields(inst_id = inst_id.unwrap_or("all"), limit))]
    pub async fn get_fills(
        &self,
        inst_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<super::models::FillDetail>> {
        #[derive(serde::Deserialize)]
        struct ResponseWrapper {
            data: Vec<super::models::FillDetail>,
        }

        let mut params = vec![("instType".to_string(), "SWAP".to_string())];
        if let Some(inst_id) = inst_id {
            params.push(("instId".to_string(), inst_id.to_string()));
        }
        if let Some(limit) = limit {
            params.push(("limit".to_string(), limit.min(100).to_string()));
        }

        let query = params
            .into_iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&");

        let path = format!("{API_PREFIX}/trade/fills?{query}");
        let response: ResponseWrapper = self.get(&path, None).await?;
        Ok(response.data)
    }

    async fn get<T>(&self, path_and_query: &str, body: Option<Value>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        tracing::debug!("OKX GET {}", path_and_query);
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

        let sign = super::models::sign_request(
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
        if self.simulated_trading {
            headers.insert("x-simulated-trading", HeaderValue::from_static("1"));
        }

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

        // Read the response body as text first
        let body = response
            .text()
            .await
            .map_err(|err| OkxError::Deserialize(err.into()))?;

        // Trim any trailing whitespace or newlines that might cause deserialization issues
        let body = body.trim();

        // Log the raw response for debugging
        tracing::debug!("OKX response body: {}", body);

        // Deserialize from the trimmed text
        serde_json::from_str::<T>(body).map_err(|err| OkxError::Deserialize(err.into()).into())
    }
}

fn current_timestamp_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
