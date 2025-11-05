use anyhow::Result;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{error, info, instrument};

use crate::config::Config;
use crate::route53::Route53Client;
use crate::s3::S3Client;

/// URL refresh scheduler
pub struct Scheduler {
    s3_client: Arc<S3Client>,
    route53_client: Arc<Route53Client>,
    config: Arc<Config>,
}

impl Scheduler {
    pub fn new(s3_client: S3Client, route53_client: Route53Client, config: Config) -> Self {
        Self {
            s3_client: Arc::new(s3_client),
            route53_client: Arc::new(route53_client),
            config: Arc::new(config),
        }
    }

    /// Run the scheduler to periodically refresh the presigned URL
    #[instrument(skip(self))]
    pub async fn run(&self) -> Result<()> {
        info!(
            "Starting scheduler with refresh interval: {:?}",
            self.config.refresh_interval
        );

        // Perform initial refresh
        self.refresh_url().await?;

        // Set up periodic refresh
        let mut interval = interval(self.config.refresh_interval);
        interval.tick().await; // First tick completes immediately

        loop {
            interval.tick().await;
            if let Err(e) = self.refresh_url().await {
                error!("Failed to refresh URL: {}", e);
                // Continue running even if refresh fails
            }
        }
    }

    /// Refresh the presigned URL and update Route53
    #[instrument(skip(self))]
    async fn refresh_url(&self) -> Result<()> {
        info!("Refreshing presigned URL");

        let (bucket, key) = self.config.parse_s3_url()?;

        // Generate new presigned URL
        let presigned_url = self
            .s3_client
            .generate_presigned_url(&bucket, &key, self.config.presign_duration)
            .await?;

        // Update Route53 DNS record
        self.route53_client
            .update_dns_record(
                &self.config.hosted_zone_id,
                &self.config.short_url,
                &presigned_url,
            )
            .await?;

        info!("Successfully refreshed presigned URL");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scheduler_creation() {
        let config = Config::new(
            "s3://test-bucket/test-key".to_string(),
            "short.example.com".to_string(),
            "Z1234567890ABC".to_string(),
        )
        .unwrap();

        let aws_config = aws_config::from_env().load().await;
        let s3_client = S3Client::new(aws_sdk_s3::Client::new(&aws_config));
        let route53_client = Route53Client::new(aws_sdk_route53::Client::new(&aws_config));

        let scheduler = Scheduler::new(s3_client, route53_client, config);

        // Just verify we can create the scheduler
        assert!(std::mem::size_of_val(&scheduler) > 0);
    }
}
