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
        name = "get_account_state",
        description = "查询 OKX 账户当前状态及持仓汇总"
    )]
    async fn get_account_state(
        &self,
        Parameters(request): Parameters<crate::account::AccountStateRequest>,
    ) -> Result<Json<crate::account::AccountState>, McpError> {
        let credentials = self
            .config
            .require_okx_credentials()
            .map_err(|err| {
                error!(%err, "缺少 OKX 凭证");
                McpError::invalid_request(format!("缺少 OKX 凭证: {}", err), None)
            })?
            .clone();

        info!(
            endpoint = %self.config.okx_rest_endpoint,
            api_key_length = credentials.api_key.len(),
            simulated = request.simulated_trading,
            "OKX 凭证加载成功"
        );

        let client = if request.simulated_trading {
            OkxRestClient::new_simulated(self.config.okx_rest_endpoint.clone(), credentials)
        } else {
            OkxRestClient::new(self.config.okx_rest_endpoint.clone(), credentials)
        }
        .map_err(|err| {
            error!(?err, "初始化 OKX 客户端失败");
            McpError::internal_error(format!("初始化 OKX 客户端失败: {}", err), None)
        })?;

        let account_state = crate::account::fetch_account_state(&client, &request)
            .await
            .map_err(|err| {
                error!(%err, "获取 OKX 账户状态失败");
                McpError::internal_error(
                    "获取 OKX 账户状态失败",
                    Some(json!({ "reason": err.to_string() })),
                )
            })?;

        Ok(Json(account_state))
    }
}

#[tool_handler]
impl ServerHandler for DemoArithmeticServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some("Demo arithmetic server with a single `one_plus_one` tool.".into()),
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
}
