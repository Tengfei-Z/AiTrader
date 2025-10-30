use ai_core::config::AppConfig;
use anyhow::{anyhow, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing::{debug, instrument};

use crate::types::{McpRequest, McpResponse};

pub struct McpProcessHandle {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl McpProcessHandle {
    #[instrument(skip_all)]
    pub async fn spawn_from_app_config(config: &AppConfig) -> Result<Self> {
        let process = config.require_mcp_config()?.clone();
        Self::spawn(process).await
    }

    #[instrument(skip_all)]
    pub async fn spawn(process: ai_core::config::McpConfig) -> Result<Self> {
        let mut command = Command::new(&process.executable);
        command.args(&process.args);
        command.kill_on_drop(true);
        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());

        let mut child = command.spawn().map_err(|err| {
            anyhow!(
                "failed to spawn MCP process {}: {}",
                process.executable,
                err
            )
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("MCP process does not expose stdin"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("MCP process does not expose stdout"))?;

        Ok(Self {
            _child: child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    pub async fn send(&mut self, request: McpRequest) -> Result<()> {
        let payload = serde_json::to_string(&request)?;
        self.stdin.write_all(payload.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    pub async fn read_stdout(&mut self) -> Result<Option<McpResponse>> {
        let mut buffer = String::new();
        let bytes = self.stdout.read_line(&mut buffer).await?;

        if bytes == 0 {
            debug!("MCP process exited or produced no output");
            return Ok(None);
        }

        let response = serde_json::from_str::<McpResponse>(&buffer)?;
        Ok(Some(response))
    }
}
