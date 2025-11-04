use ai_core::config::AppConfig;
use anyhow::{anyhow, Result};
use okx::OkxRestClient;
use rmcp::{
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{CallToolResult, Content, ProtocolVersion, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler, ServiceExt,
};
use serde_json::json;
use tracing::{error, info};

/// Small demo service that exposes a single `one_plus_one` MCP tool.
///
/// This allows us to verify the `rmcp` integration locally (e.g. via the MCP inspector CLI)
/// before we build out the real tool surface.
#[derive(Clone)]
pub struct DemoArithmeticServer {
    config: AppConfig,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl DemoArithmeticServer {
    /// Construct a new demo server instance with its router wiring initialised.
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            tool_router: Self::tool_router(),
        }
    }

    /// Return the result of `1 + 1` as plain text content.
    #[tool(name = "one_plus_one", description = "Return the answer to 1 + 1.")]
    async fn one_plus_one(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text("2")]))
    }

    #[tool(
        name = "get_market_data",
        description = "获取指定合约的实时行情、技术指标及资金费率等数据"
    )]
    async fn get_market_data(
        &self,
        Parameters(request): Parameters<crate::market::MarketDataRequest>,
    ) -> Result<Json<crate::market::MarketDataResponse>, McpError> {
        let coins_preview = request.coins.join(",");
        info!(
            tool = "get_market_data",
            coins = %coins_preview,
            timeframe = %request.timeframe,
            quote = %request.quote,
            include_orderbook = request.include_orderbook,
            include_funding = request.include_funding,
            include_open_interest = request.include_open_interest,
            simulated = request.simulated_trading,
            "Received MCP tool request"
        );
        let client = self.build_okx_client(request.simulated_trading)?;
        let response = crate::market::fetch_market_data(&client, &request)
            .await
            .map_err(|err| {
                error!(%err, "获取行情数据失败");
                McpError::internal_error(
                    "获取行情数据失败",
                    Some(json!({ "reason": err.to_string() })),
                )
            })?;

        info!(
            tool = "get_market_data",
            coins_count = response.coins.len(),
            "Returning MCP tool response"
        );

        Ok(Json(response))
    }
    #[tool(
        name = "get_account_state",
        description = "查询 OKX 账户当前状态及持仓汇总"
    )]
    async fn get_account_state(
        &self,
        Parameters(request): Parameters<crate::account::AccountStateRequest>,
    ) -> Result<Json<crate::account::AccountState>, McpError> {
        info!(
            tool = "get_account_state",
            simulated = %request.simulated_trading,
            "Received MCP tool request"
        );
        let client = self.build_okx_client(request.simulated_trading)?;

        let account_state = crate::account::fetch_account_state(&client, &request)
            .await
            .map_err(|err| {
                error!(%err, "获取 OKX 账户状态失败");
                McpError::internal_error(
                    "获取 OKX 账户状态失败",
                    Some(json!({ "reason": err.to_string() })),
                )
            })?;

        info!(
            tool = "get_account_state",
            simulated = %request.simulated_trading,
            "Returning MCP tool response"
        );

        Ok(Json(account_state))
    }
    #[tool(
        name = "execute_trade",
        description = "执行 OKX 开仓或平仓操作（永续合约默认使用 cross 保证金模式）"
    )]
    async fn execute_trade(
        &self,
        Parameters(request): Parameters<crate::trade::ExecuteTradeRequest>,
    ) -> Result<Json<crate::trade::ExecuteTradeResponse>, McpError> {
        let instrument = request
            .instrument_id
            .as_deref()
            .unwrap_or_else(|| &request.coin);
        info!(
            tool = "execute_trade",
            action = ?request.action,
            instrument = %instrument,
            leverage = ?request.leverage,
            margin_amount = ?request.margin_amount,
            quantity = ?request.quantity,
            simulated = request.simulated_trading,
            "Received MCP tool request"
        );
        let client = self.build_okx_client(request.simulated_trading)?;
        let response = crate::trade::execute_trade(&client, &request)
            .await
            .map_err(|err| {
                error!(
                    %err,
                    instrument = request.instrument_id.as_deref().unwrap_or(&request.coin),
                    "执行交易失败"
                );
                McpError::internal_error("执行交易失败", Some(json!({ "reason": err.to_string() })))
            })?;

        let order_id = response.order_id.as_deref().unwrap_or("unknown");
        info!(
            tool = "execute_trade",
            order_id = %order_id,
            instrument = %instrument,
            success = response.success,
            "Returning MCP tool response"
        );

        Ok(Json(response))
    }

    #[tool(name = "update_exit_plan", description = "更新已有仓位的止盈止损计划")]
    async fn update_exit_plan(
        &self,
        Parameters(request): Parameters<crate::trade::UpdateExitPlanRequest>,
    ) -> Result<Json<crate::trade::UpdateExitPlanResponse>, McpError> {
        info!(
            tool = "update_exit_plan",
            position_id = %request.position_id,
            simulated = %request.simulated_trading,
            "Received MCP tool request"
        );
        let client = self.build_okx_client(request.simulated_trading)?;
        let response = crate::trade::update_exit_plan(&client, &request)
            .await
            .map_err(|err| {
                error!(
                    %err,
                    position_id = %request.position_id,
                    "更新退出计划失败"
                );
                McpError::internal_error(
                    "更新退出计划失败",
                    Some(json!({ "reason": err.to_string() })),
                )
            })?;

        info!(
            tool = "update_exit_plan",
            position_id = %request.position_id,
            "Returning MCP tool response"
        );

        Ok(Json(response))
    }
}

#[tool_handler]
impl ServerHandler for DemoArithmeticServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "AiTrader MCP server exposing arithmetic demo plus OKX account and trading tools."
                    .into(),
            ),
            ..ServerInfo::default()
        }
    }
}

impl DemoArithmeticServer {
    /// Run the demo server over stdio transport and wait until the peer disconnects.
    pub async fn serve_stdio(self) -> Result<()> {
        let service = self
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|err| anyhow!(err))?;

        service.waiting().await.map_err(|err| anyhow!(err))?;

        Ok(())
    }

    fn build_okx_client(&self, simulated: bool) -> Result<OkxRestClient, McpError> {
        let endpoint = self.config.okx_rest_endpoint.clone();

        if simulated {
            let credentials = self
                .config
                .require_okx_simulated_credentials()
                .map_err(|err| {
                    error!(%err, "缺少 OKX 模拟账户凭证");
                    McpError::invalid_request(format!("缺少 OKX 模拟账户凭证: {}", err), None)
                })?
                .clone();

            info!(
                endpoint = %endpoint,
                api_key_length = credentials.api_key.len(),
                simulated = true,
                "OKX 模拟凭证加载成功"
            );

            OkxRestClient::new_simulated(endpoint.clone(), credentials).map_err(|err| {
                error!(?err, "初始化 OKX 模拟客户端失败");
                McpError::internal_error(format!("初始化 OKX 模拟客户端失败: {}", err), None)
            })
        } else {
            let credentials = self
                .config
                .require_okx_credentials()
                .map_err(|err| {
                    error!(%err, "缺少 OKX 凭证");
                    McpError::invalid_request(format!("缺少 OKX 凭证: {}", err), None)
                })?
                .clone();

            info!(
                endpoint = %endpoint,
                api_key_length = credentials.api_key.len(),
                simulated = false,
                "OKX 凭证加载成功"
            );

            OkxRestClient::new(endpoint.clone(), credentials).map_err(|err| {
                error!(?err, "初始化 OKX 客户端失败");
                McpError::internal_error(format!("初始化 OKX 客户端失败: {}", err), None)
            })
        }
    }
}
