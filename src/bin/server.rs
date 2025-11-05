use anyhow::Result;
use s3_buddy::{MappingManager, Route53Client, S3Client};
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

    // Get proxy hostname from environment (required)
    let proxy_hostname = std::env::var("PROXY_HOSTNAME").unwrap_or_else(|_| {
        eprintln!("ERROR: PROXY_HOSTNAME environment variable is required");
        eprintln!("Example: PROXY_HOSTNAME=proxy.example.com");
        std::process::exit(1);
    });

    info!("Proxy hostname: {}", proxy_hostname);

    // Load AWS configuration
    let aws_config = aws_config::load_from_env().await;

    // Create AWS clients
    let s3_client = S3Client::new(aws_sdk_s3::Client::new(&aws_config));
    let s3_client = Arc::new(s3_client);
    let route53_client = Route53Client::new(aws_sdk_route53::Client::new(&aws_config));

    // Create mapping manager
    let (manager, mut log_rx) = MappingManager::new(route53_client, proxy_hostname);
    let manager = Arc::new(manager);

    // Spawn task to handle logs
    tokio::spawn(async move {
        while let Some(log) = log_rx.recv().await {
            if log.success {
                info!("[{}] {}", log.mapping_id, log.message);
            } else {
                tracing::error!("[{}] {}", log.mapping_id, log.message);
            }
        }
    });

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
