use super::error::OkxError;
use crate::settings::{AppConfig, OkxCredentials};
use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use reqwest::{Client, Method, RequestBuilder};
use serde::de::DeserializeOwned;
use serde_json::{self, Value};
use tracing::{instrument, warn};
use url::form_urlencoded;

/// OKX API v5 的路径前缀
const API_PREFIX: &str = "/api/v5";

/// OKX REST API 客户端
///
/// 用于与 OKX 交易所进行 HTTP 通信，支持获取行情、账户信息、持仓、成交记录等。
#[derive(Debug, Clone)]
pub struct OkxRestClient {
    /// HTTP 客户端
    http: Client,
    /// OKX API 基础 URL (可能是正式环境或模拟盘环境)
    base_url: String,
    /// API 认证凭据 (API Key, Secret, Passphrase)
    credentials: OkxCredentials,
    /// 是否使用模拟盘交易 (如果为 true，请求头会包含 x-simulated-trading: 1)
    simulated_trading: bool,
}

/// 代理配置选项
#[derive(Debug, Clone, Default)]
pub struct ProxyOptions {
    /// HTTP 代理地址
    pub http: Option<String>,
    /// HTTPS 代理地址
    pub https: Option<String>,
}

impl OkxRestClient {
    /// 从配置文件创建 OKX 客户端，并支持代理配置
    ///
    /// 会自动从 config 中读取：
    /// - API 凭据 (API Key, Secret, Passphrase)
    /// - 基础 URL
    /// - 是否使用模拟盘
    pub fn from_config_with_proxy(config: &AppConfig, proxy: ProxyOptions) -> Result<Self> {
        let credentials = config.require_okx_credentials()?.clone();
        Self::new_with_proxy(
            config.okx_base_url.clone(),
            credentials,
            proxy,
            config.okx_use_simulated(),
        )
    }

    /// 使用指定参数创建 OKX 客户端（内部方法）
    ///
    /// 会配置 HTTP 客户端的代理设置
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

    /// 获取指定交易对的实时行情 Ticker 数据
    ///
    /// 包含最新价格、24小时成交量、最高价、最低价等信息
    ///
    /// # 参数
    /// - `inst_id`: 产品ID，例如 "BTC-USDT-SWAP" (BTC 永续合约)
    #[instrument(skip(self), fields(inst_id = %inst_id))]
    pub async fn get_ticker(&self, inst_id: &str) -> Result<super::models::Ticker> {
        #[derive(serde::Deserialize)]
        struct TickerResponse {
            data: Vec<super::models::Ticker>,
        }

        let path = format!("{API_PREFIX}/market/ticker?instId={}", inst_id);
        let response: TickerResponse = self.get(&path, None).await?;

        if response.data.is_empty() {
            tracing::warn!("获取 ticker 数据为空: {}", inst_id);
        } else {
            tracing::trace!("成功获取 {} 的 ticker 数据", inst_id);
        }

        response
            .data
            .into_iter()
            .next()
            .ok_or_else(|| OkxError::EmptyResponse("market/ticker".into()).into())
    }

    /// 获取账户余额信息
    ///
    /// 返回账户的资产余额、可用余额、冻结金额、权益等信息
    #[instrument(skip(self))]
    pub async fn get_account_balance(&self) -> Result<super::models::AccountBalanceResponse> {
        let path = format!("{API_PREFIX}/account/balance");
        let response: super::models::AccountBalanceResponse = self.get(&path, None).await?;
        tracing::trace!("成功获取账户余额信息");
        Ok(response)
    }

    /// 获取持仓信息
    ///
    /// 返回当前账户的所有持仓明细，包括持仓方向、数量、未实现盈亏等
    ///
    /// # 参数
    /// - `inst_type`: 可选的产品类型过滤，例如 "SWAP" (永续合约)、"FUTURES" (交割合约) 等
    ///                如果为 None，则返回所有类型的持仓
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
        tracing::info!("获取到 {} 条持仓记录", response.data.len());
        Ok(response.data)
    }

    /// 获取订单历史
    pub async fn get_order_history(
        &self,
        inst_type: Option<&str>,
        inst_id: Option<&str>,
        state: Option<&str>,
        ord_id: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<super::models::OrderHistoryEntry>> {
        let mut path = format!("{API_PREFIX}/trade/orders-history");
        append_query_param_if_some(&mut path, "instType", inst_type);
        append_query_param_if_some(&mut path, "instId", inst_id);
        append_query_param_if_some(&mut path, "state", state);
        append_query_param_if_some(&mut path, "ordId", ord_id);
        if let Some(limit_value) = limit {
            append_query_param(&mut path, "limit", &limit_value.to_string());
        }

        let response: super::models::OrderHistoryResponse = self.get(&path, None).await?;
        Ok(response.data)
    }

    /// 获取成交回报（fills）
    pub async fn get_fills(
        &self,
        inst_id: Option<&str>,
        ord_id: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<super::models::FillDetail>> {
        let mut path = format!("{API_PREFIX}/trade/fills");
        append_query_param_if_some(&mut path, "instId", inst_id);
        append_query_param_if_some(&mut path, "ordId", ord_id);
        if let Some(limit_value) = limit {
            append_query_param(&mut path, "limit", &limit_value.to_string());
        }

        let response: super::models::FillResponse = self.get(&path, None).await?;
        Ok(response.data)
    }

    /// 内部方法：发送 GET 请求并反序列化响应
    async fn get<T>(&self, path_and_query: &str, body: Option<Value>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        tracing::debug!("OKX GET {}", path_and_query);
        let builder = self.prepare_request(Method::GET, path_and_query, body)?;
        self.execute(builder).await
    }

    /// 准备 HTTP 请求，添加 OKX API 所需的签名和认证头
    ///
    /// 包括：
    /// - 生成时间戳
    /// - 计算请求签名
    /// - 设置认证头 (OK-ACCESS-KEY, OK-ACCESS-SIGN, OK-ACCESS-TIMESTAMP 等)
    /// - 如果是模拟盘，添加 x-simulated-trading 头
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

    /// 执行 HTTP 请求并处理响应
    ///
    /// 会检查 HTTP 状态码，读取响应体，去除空白字符，然后反序列化为目标类型
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

        // 先读取响应体为文本
        let body = response
            .text()
            .await
            .map_err(|err| OkxError::Deserialize(err.into()))?;

        // 去除尾部空白字符或换行符，避免反序列化问题
        let body = body.trim();

        // 记录原始响应用于调试
        tracing::debug!("OKX response body: {}", body);

        match serde_json::from_str::<T>(body) {
            Ok(parsed) => Ok(parsed),
            Err(err) => {
                warn!(
                    error = ?err,
                    response_body = %body,
                    "failed to deserialize OKX response"
                );
                Err(OkxError::Deserialize(err.into()).into())
            }
        }
    }
}

/// 生成当前时间的 ISO 8601 格式时间戳（毫秒精度）
///
/// OKX API 要求使用此格式的时间戳进行签名
fn current_timestamp_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn append_query_param_if_some(path: &mut String, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        append_query_param(path, key, value);
    }
}

fn append_query_param(path: &mut String, key: &str, value: &str) {
    if path.contains('?') {
        path.push('&');
    } else {
        path.push('?');
    }

    path.push_str(key);
    path.push('=');
    path.push_str(&form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>());
}
