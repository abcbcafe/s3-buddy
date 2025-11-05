use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, instrument};
use uuid::Uuid;

use crate::route53::Route53Client;
use crate::types::{Mapping, MappingStatus, RefreshLog};

/// Manages multiple URL mappings
/// Mappings are now handled via proxy, so no periodic refresh needed
pub struct MappingManager {
    mappings: Arc<RwLock<HashMap<Uuid, Mapping>>>,
    route53_client: Arc<Route53Client>,
    proxy_hostname: String,
    log_tx: mpsc::UnboundedSender<RefreshLog>,
}

impl MappingManager {
    pub fn new(
        route53_client: Route53Client,
        proxy_hostname: String,
    ) -> (Self, mpsc::UnboundedReceiver<RefreshLog>) {
        let (log_tx, log_rx) = mpsc::unbounded_channel();

        (
            Self {
                mappings: Arc::new(RwLock::new(HashMap::new())),
                route53_client: Arc::new(route53_client),
                proxy_hostname,
                log_tx,
            },
            log_rx,
        )
    }

    /// Add a new mapping and configure DNS
    #[instrument(skip(self))]
    pub async fn add_mapping(&self, mut mapping: Mapping) -> Result<Uuid> {
        let id = mapping.id;
        info!(
            "Adding mapping: {} -> {} (proxy: {})",
            mapping.s3_url, mapping.short_url, self.proxy_hostname
        );

        // Validate S3 URL format
        if !mapping.s3_url.starts_with("s3://") {
            anyhow::bail!("S3 URL must start with s3://");
        }

        // Configure DNS to point to proxy
        self.route53_client
            .configure_dns_for_proxy(
                &mapping.hosted_zone_id,
                &mapping.short_url,
                &self.proxy_hostname,
            )
            .await
            .context("Failed to configure DNS")?;

        mapping.status = MappingStatus::Active;
        mapping.dns_configured_at = Some(Utc::now());
        mapping.updated_at = Utc::now();

        // Store the mapping
        {
            let mut mappings = self.mappings.write().await;
            mappings.insert(id, mapping.clone());
        }

        // Log success
        let _ = self.log_tx.send(RefreshLog {
            mapping_id: id,
            timestamp: Utc::now(),
            success: true,
            message: "DNS configured successfully".to_string(),
        });

        Ok(id)
    }

    /// Get a mapping by ID
    pub async fn get_mapping(&self, id: &Uuid) -> Option<Mapping> {
        let mappings = self.mappings.read().await;
        mappings.get(id).cloned()
    }

    /// List all mappings
    pub async fn list_mappings(&self) -> Vec<Mapping> {
        let mappings = self.mappings.read().await;
        mappings.values().cloned().collect()
    }

    /// Update a mapping
    #[instrument(skip(self))]
    pub async fn update_mapping(&self, id: &Uuid, mut updates: Mapping) -> Result<()> {
        info!("Updating mapping {}", id);

        // If DNS configuration has changed, update it
        let old_mapping = self.get_mapping(id).await.context("Mapping not found")?;

        if old_mapping.short_url != updates.short_url
            || old_mapping.hosted_zone_id != updates.hosted_zone_id
        {
            self.route53_client
                .configure_dns_for_proxy(
                    &updates.hosted_zone_id,
                    &updates.short_url,
                    &self.proxy_hostname,
                )
                .await
                .context("Failed to update DNS configuration")?;

            updates.dns_configured_at = Some(Utc::now());
        }

        // Update the mapping
        {
            let mut mappings = self.mappings.write().await;
            if let Some(mapping) = mappings.get_mut(id) {
                *mapping = updates;
                mapping.updated_at = Utc::now();
            } else {
                anyhow::bail!("Mapping not found");
            }
        }

        Ok(())
    }

    /// Delete a mapping
    #[instrument(skip(self))]
    pub async fn delete_mapping(&self, id: &Uuid) -> Result<()> {
        info!("Deleting mapping {}", id);

        // Remove from storage
        let mut mappings = self.mappings.write().await;
        mappings.remove(id).context("Mapping not found")?;

        Ok(())
    }

    /// Pause a mapping (disable proxy handling)
    #[instrument(skip(self))]
    pub async fn pause_mapping(&self, id: &Uuid) -> Result<()> {
        info!("Pausing mapping {}", id);

        let mut mappings = self.mappings.write().await;
        if let Some(mapping) = mappings.get_mut(id) {
            mapping.status = MappingStatus::Paused;
            mapping.updated_at = Utc::now();
        } else {
            anyhow::bail!("Mapping not found");
        }

        Ok(())
    }

    /// Resume a paused mapping (enable proxy handling)
    #[instrument(skip(self))]
    pub async fn resume_mapping(&self, id: &Uuid) -> Result<()> {
        info!("Resuming mapping {}", id);

        let mut mappings = self.mappings.write().await;
        if let Some(mapping) = mappings.get_mut(id) {
            mapping.status = MappingStatus::Active;
            mapping.updated_at = Utc::now();
        } else {
            anyhow::bail!("Mapping not found");
        }

        Ok(())
    }
}
