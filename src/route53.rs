use anyhow::{Context, Result};
use aws_sdk_route53::types::{
    Change, ChangeAction, ChangeBatch, ResourceRecord, ResourceRecordSet, RrType,
};
use tracing::{info, instrument};

/// Route53 client wrapper for DNS operations
pub struct Route53Client {
    client: aws_sdk_route53::Client,
}

impl Route53Client {
    pub fn new(client: aws_sdk_route53::Client) -> Self {
        Self { client }
    }

    /// Create or update a CNAME record pointing to the proxy server
    #[instrument(skip(self))]
    pub async fn configure_dns_for_proxy(
        &self,
        hosted_zone_id: &str,
        short_url: &str,
        proxy_hostname: &str,
    ) -> Result<()> {
        info!(
            "Configuring DNS record {} to point to proxy {}",
            short_url, proxy_hostname
        );

        // CNAME records need a trailing dot
        let target = format!("{}.", proxy_hostname.trim_end_matches('.'));

        let resource_record = ResourceRecord::builder()
            .value(target)
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
            .comment("Configured by s3-buddy proxy")
            .build()
            .context("Failed to build change batch")?;

        self.client
            .change_resource_record_sets()
            .hosted_zone_id(hosted_zone_id)
            .change_batch(change_batch)
            .send()
            .await
            .context("Failed to update Route53 record")?;

        info!("Successfully configured DNS record {}", short_url);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_route53_client_creation() {
        let config = aws_config::from_env().load().await;
        let client = aws_sdk_route53::Client::new(&config);
        let route53_client = Route53Client::new(client);

        // Just verify we can create the client
        assert!(std::mem::size_of_val(&route53_client) > 0);
    }
}
