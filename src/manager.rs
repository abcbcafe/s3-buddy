use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{error, info, instrument};
use uuid::Uuid;

use crate::config::Config;
use crate::route53::Route53Client;
use crate::s3::S3Client;
use crate::types::{Mapping, MappingStatus, RefreshLog};

/// Manages multiple URL mappings and their refresh schedulers
pub struct MappingManager {
    mappings: Arc<RwLock<HashMap<Uuid, Mapping>>>,
    tasks: Arc<RwLock<HashMap<Uuid, JoinHandle<()>>>>,
    s3_client: Arc<S3Client>,
    route53_client: Arc<Route53Client>,
    log_tx: mpsc::UnboundedSender<RefreshLog>,
}

impl MappingManager {
    pub fn new(
        s3_client: S3Client,
        route53_client: Route53Client,
    ) -> (Self, mpsc::UnboundedReceiver<RefreshLog>) {
        let (log_tx, log_rx) = mpsc::unbounded_channel();

        (
            Self {
                mappings: Arc::new(RwLock::new(HashMap::new())),
                tasks: Arc::new(RwLock::new(HashMap::new())),
                s3_client: Arc::new(s3_client),
                route53_client: Arc::new(route53_client),
                log_tx,
            },
            log_rx,
        )
    }

    /// Add a new mapping and start its refresh scheduler
    #[instrument(skip(self))]
    pub async fn add_mapping(&self, mut mapping: Mapping) -> Result<Uuid> {
        let id = mapping.id;
        info!(
            "Adding mapping: {} -> {}",
            mapping.s3_url, mapping.short_url
        );

        // Validate S3 URL format
        if !mapping.s3_url.starts_with("s3://") {
            anyhow::bail!("S3 URL must start with s3://");
        }

        mapping.status = MappingStatus::Active;
        mapping.updated_at = Utc::now();

        // Store the mapping
        {
            let mut mappings = self.mappings.write().await;
            mappings.insert(id, mapping.clone());
        }

        // Start the refresh task - if this fails, remove the mapping
        if let Err(e) = self.start_refresh_task(mapping).await {
            let mut mappings = self.mappings.write().await;
            mappings.remove(&id);
            return Err(e);
        }

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
    pub async fn update_mapping(&self, id: &Uuid, updates: Mapping) -> Result<()> {
        info!("Updating mapping {}", id);

        // Stop the existing task
        self.stop_refresh_task(id).await;

        // Update the mapping
        {
            let mut mappings = self.mappings.write().await;
            if let Some(mapping) = mappings.get_mut(id) {
                *mapping = updates.clone();
                mapping.updated_at = Utc::now();
            } else {
                anyhow::bail!("Mapping not found");
            }
        }

        // Restart the task if active
        if updates.status == MappingStatus::Active {
            self.start_refresh_task(updates).await?;
        }

        Ok(())
    }

    /// Delete a mapping
    #[instrument(skip(self))]
    pub async fn delete_mapping(&self, id: &Uuid) -> Result<()> {
        info!("Deleting mapping {}", id);

        // Stop the refresh task
        self.stop_refresh_task(id).await;

        // Remove from storage
        let mut mappings = self.mappings.write().await;
        mappings.remove(id).context("Mapping not found")?;

        Ok(())
    }

    /// Pause a mapping (stop refreshing)
    #[instrument(skip(self))]
    pub async fn pause_mapping(&self, id: &Uuid) -> Result<()> {
        info!("Pausing mapping {}", id);

        self.stop_refresh_task(id).await;

        let mut mappings = self.mappings.write().await;
        if let Some(mapping) = mappings.get_mut(id) {
            mapping.status = MappingStatus::Paused;
            mapping.updated_at = Utc::now();
        } else {
            anyhow::bail!("Mapping not found");
        }

        Ok(())
    }

    /// Resume a paused mapping
    #[instrument(skip(self))]
    pub async fn resume_mapping(&self, id: &Uuid) -> Result<()> {
        info!("Resuming mapping {}", id);

        let mapping = {
            let mut mappings = self.mappings.write().await;
            if let Some(mapping) = mappings.get_mut(id) {
                mapping.status = MappingStatus::Active;
                mapping.updated_at = Utc::now();
                mapping.clone()
            } else {
                anyhow::bail!("Mapping not found");
            }
        };

        self.start_refresh_task(mapping).await?;

        Ok(())
    }

    /// Start a refresh task for a mapping
    async fn start_refresh_task(&self, mapping: Mapping) -> Result<()> {
        let id = mapping.id;
        let mappings = Arc::clone(&self.mappings);
        let s3_client = Arc::clone(&self.s3_client);
        let route53_client = Arc::clone(&self.route53_client);
        let log_tx = self.log_tx.clone();

        let handle = tokio::spawn(async move {
            let refresh_interval = mapping.refresh_interval();
            let presign_duration = mapping.presign_duration();

            // Perform initial refresh
            refresh_url(
                &mapping,
                &s3_client,
                &route53_client,
                &mappings,
                presign_duration,
                &log_tx,
            )
            .await;

            // Set up periodic refresh
            let mut interval = interval(refresh_interval);
            interval.tick().await; // First tick completes immediately

            loop {
                interval.tick().await;
                refresh_url(
                    &mapping,
                    &s3_client,
                    &route53_client,
                    &mappings,
                    presign_duration,
                    &log_tx,
                )
                .await;
            }
        });

        let mut tasks = self.tasks.write().await;
        tasks.insert(id, handle);

        Ok(())
    }

    /// Stop a refresh task for a mapping
    async fn stop_refresh_task(&self, id: &Uuid) {
        let mut tasks = self.tasks.write().await;
        if let Some(handle) = tasks.remove(id) {
            handle.abort();
        }
    }
}

/// Refresh the presigned URL and update Route53
#[instrument(skip(s3_client, route53_client, mappings, log_tx))]
async fn refresh_url(
    mapping: &Mapping,
    s3_client: &S3Client,
    route53_client: &Route53Client,
    mappings: &Arc<RwLock<HashMap<Uuid, Mapping>>>,
    presign_duration: std::time::Duration,
    log_tx: &mpsc::UnboundedSender<RefreshLog>,
) {
    info!("Refreshing presigned URL for {}", mapping.id);

    let result = async {
        // Parse S3 URL
        let config = Config::new(
            mapping.s3_url.clone(),
            mapping.short_url.clone(),
            mapping.hosted_zone_id.clone(),
        )?;
        let (bucket, key) = config.parse_s3_url()?;

        // Generate new presigned URL
        let presigned_url = s3_client
            .generate_presigned_url(&bucket, &key, presign_duration)
            .await?;

        // Update Route53 DNS record
        route53_client
            .update_dns_record(&mapping.hosted_zone_id, &mapping.short_url, &presigned_url)
            .await?;

        Ok::<_, anyhow::Error>(())
    }
    .await;

    // Update mapping status
    let mut mappings = mappings.write().await;
    if let Some(stored_mapping) = mappings.get_mut(&mapping.id) {
        match result {
            Ok(_) => {
                stored_mapping.last_refresh = Some(Utc::now());
                stored_mapping.next_refresh = Some(
                    Utc::now() + chrono::Duration::from_std(mapping.refresh_interval()).unwrap(),
                );
                stored_mapping.status = MappingStatus::Active;
                stored_mapping.last_error = None;

                let _ = log_tx.send(RefreshLog {
                    mapping_id: mapping.id,
                    timestamp: Utc::now(),
                    success: true,
                    message: "Successfully refreshed presigned URL".to_string(),
                });

                info!("Successfully refreshed presigned URL for {}", mapping.id);
            }
            Err(e) => {
                let error_msg = format!("Failed to refresh URL: {}", e);
                stored_mapping.status = MappingStatus::Error;
                stored_mapping.last_error = Some(error_msg.clone());

                let _ = log_tx.send(RefreshLog {
                    mapping_id: mapping.id,
                    timestamp: Utc::now(),
                    success: false,
                    message: error_msg.clone(),
                });

                error!("Failed to refresh presigned URL for {}: {}", mapping.id, e);
            }
        }
    }
}
