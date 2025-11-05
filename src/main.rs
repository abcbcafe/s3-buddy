use anyhow::{Context, Result};
use s3_buddy::{Config, Route53Client, S3Client, Scheduler};
use std::env;
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

    info!("Starting S3 Buddy");

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: {} <s3-url> <short-url> <hosted-zone-id>", args[0]);
        eprintln!("Example: {} s3://my-bucket/file.txt short.example.com Z1234567890ABC", args[0]);
        std::process::exit(1);
    }

    let s3_url = args[1].clone();
    let short_url = args[2].clone();
    let hosted_zone_id = args[3].clone();

    // Create configuration
    let config = Config::new(s3_url, short_url, hosted_zone_id)
        .context("Failed to create configuration")?;

    info!("Configuration loaded: {:?}", config);

    // Load AWS configuration from environment/credentials
    let aws_config = aws_config::load_from_env().await;

    // Create AWS clients
    let s3_client = S3Client::new(aws_sdk_s3::Client::new(&aws_config));
    let route53_client = Route53Client::new(aws_sdk_route53::Client::new(&aws_config));

    // Create and run scheduler
    let scheduler = Scheduler::new(s3_client, route53_client, config);

    info!("Starting URL refresh scheduler");
    scheduler.run().await?;

    Ok(())
}
