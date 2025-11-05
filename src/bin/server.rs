use anyhow::Result;
use s3_buddy::{MappingManager, S3Client};
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("Starting S3 Buddy Server");

    // Load AWS configuration
    let aws_config = aws_config::load_from_env().await;

    // Create AWS clients
    let s3_client = S3Client::new(aws_sdk_s3::Client::new(&aws_config));
    let s3_client = Arc::new(s3_client);

    // Create mapping manager
    let manager = Arc::new(MappingManager::new());

    // Create HTTP server
    let app = s3_buddy::server::create_router(manager, s3_client);

    // Get port from environment or use default
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = format!("0.0.0.0:{}", port);
    info!("Server listening on {}", addr);

    // Start server
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
