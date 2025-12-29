//! MCP module - Direct access to osu! data for AI assistants

mod tools;
mod types;

pub use tools::OsuSyncTools;
use rmcp::ServiceExt;

pub async fn run_mcp_server() -> anyhow::Result<()> {
    let server = OsuSyncTools::new().serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
