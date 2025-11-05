use anyhow::{Context, Result};
use aws_sdk_route53::types::{Change, ChangeAction, ChangeBatch, ResourceRecord, ResourceRecordSet, RrType};
use tracing::{info, instrument};

/// Route53 client wrapper for DNS operations
pub struct Route53Client {
    client: aws_sdk_route53::Client,
}

impl Route53Client {
    pub fn new(client: aws_sdk_route53::Client) -> Self {
        Self { client }
    }

    /// Update or create a CNAME record pointing to the presigned URL
    #[instrument(skip(self, presigned_url))]
    pub async fn update_dns_record(
        &self,
        hosted_zone_id: &str,
        short_url: &str,
        presigned_url: &str,
    ) -> Result<()> {
        info!(
            "Updating DNS record {} to point to presigned URL",
            short_url
        );

        // Extract the hostname from the presigned URL
        let target_url = Self::extract_hostname(presigned_url)?;

        let resource_record = ResourceRecord::builder()
            .value(target_url)
            .build()
            .context("Failed to build resource record")?;

        let record_set = ResourceRecordSet::builder()
            .name(short_url)
            .r#type(RrType::Cname)
            .ttl(300) // 5 minutes TTL
            .resource_records(resource_record)
            .build()
            .context("Failed to build record set")?;

        let change = Change::builder()
            .action(ChangeAction::Upsert)
            .resource_record_set(record_set)
            .build()
            .context("Failed to build change")?;

        let change_batch = ChangeBatch::builder()
            .changes(change)
            .comment("Updated by s3-buddy")
            .build()
            .context("Failed to build change batch")?;

        self.client
            .change_resource_record_sets()
            .hosted_zone_id(hosted_zone_id)
            .change_batch(change_batch)
            .send()
            .await
            .context("Failed to update Route53 record")?;

        info!("Successfully updated DNS record {}", short_url);

        Ok(())
    }

    /// Extract hostname from presigned URL for CNAME target
    fn extract_hostname(url: &str) -> Result<String> {
        let parsed = url::Url::parse(url)
            .context("Failed to parse presigned URL")?;

        let host = parsed.host_str()
            .context("No hostname in presigned URL")?;

        // CNAME records need a trailing dot
        Ok(format!("{}.", host))
    }
}

// Add url as a dependency
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_hostname() {
        let url = "https://my-bucket.s3.amazonaws.com/path/to/file?X-Amz-Signature=abc";
        let hostname = Route53Client::extract_hostname(url).unwrap();
        assert_eq!(hostname, "my-bucket.s3.amazonaws.com.");
    }

    #[tokio::test]
    async fn test_route53_client_creation() {
        let config = aws_config::from_env().load().await;
        let client = aws_sdk_route53::Client::new(&config);
        let route53_client = Route53Client::new(client);

        // Just verify we can create the client
        assert!(std::mem::size_of_val(&route53_client) > 0);
    }
}
