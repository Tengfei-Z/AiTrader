pub mod account;
pub mod process;
pub mod server;
pub mod types;

pub use process::McpProcessHandle;
pub use server::DemoArithmeticServer;
pub use types::{McpRequest, McpResponse};
