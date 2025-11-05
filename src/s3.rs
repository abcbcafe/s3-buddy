use anyhow::{Context, Result};
use aws_sdk_s3::presigning::PresigningConfig;
use std::time::Duration;
use tracing::{info, instrument};

/// S3 client wrapper for presigned URL operations
pub struct S3Client {
    client: aws_sdk_s3::Client,
}

impl S3Client {
    pub fn new(client: aws_sdk_s3::Client) -> Self {
        Self { client }
    }

    /// Generate a presigned URL for an S3 object
    #[instrument(skip(self))]
    pub async fn generate_presigned_url(
        &self,
        bucket: &str,
        key: &str,
        duration: Duration,
    ) -> Result<String> {
        info!(
            "Generating presigned URL for s3://{}/{} with duration {:?}",
            bucket, key, duration
        );

        let presigning_config =
            PresigningConfig::expires_in(duration).context("Failed to create presigning config")?;

        let presigned_request = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .presigned(presigning_config)
            .await
            .context("Failed to generate presigned URL")?;

        let url = presigned_request.uri().to_string();
        info!("Generated presigned URL: {}", url);

        Ok(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_s3_client_creation() {
        let config = aws_config::from_env().load().await;
        let client = aws_sdk_s3::Client::new(&config);
        let s3_client = S3Client::new(client);

        // Just verify we can create the client
        assert!(std::mem::size_of_val(&s3_client) > 0);
    }
}
