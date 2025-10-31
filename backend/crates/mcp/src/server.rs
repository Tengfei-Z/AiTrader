use anyhow::{anyhow, Result};
use rmcp::{
    handler::server::router::tool::ToolRouter,
    model::{CallToolResult, Content, ProtocolVersion, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler, ServiceExt,
};

/// Small demo service that exposes a single `one_plus_one` MCP tool.
///
/// This allows us to verify the `rmcp` integration locally (e.g. via the MCP inspector CLI)
/// before we build out the real tool surface.
#[derive(Clone)]
pub struct DemoArithmeticServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl DemoArithmeticServer {
    /// Construct a new demo server instance with its router wiring initialised.
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// Return the result of `1 + 1` as plain text content.
    #[tool(name = "one_plus_one", description = "Return the answer to 1 + 1.")]
    async fn one_plus_one(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text("2")]))
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

        service
            .waiting()
            .await
            .map_err(|err| anyhow!(err))?;

        Ok(())
    }
}
