use anyhow::Result;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    // Get server URL from environment or use default
    let server_url =
        env::var("S3_BUDDY_SERVER").unwrap_or_else(|_| "http://localhost:3000".to_string());

    // Run the TUI
    s3_buddy::tui::run_tui(server_url).await?;

    Ok(())
}
