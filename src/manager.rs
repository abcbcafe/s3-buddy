use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, instrument};
use uuid::Uuid;

use crate::types::{Mapping, MappingStatus};

/// Manages multiple URL mappings
/// Mappings are handled via HTTP proxy - users configure DNS manually
pub struct MappingManager {
    mappings: Arc<RwLock<HashMap<Uuid, Mapping>>>,
}

impl MappingManager {
    pub fn new() -> Self {
        Self {
            mappings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a new mapping
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

        info!("Mapping added successfully. Configure DNS: {} CNAME → <your-server>", mapping.short_url);

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

        updates.updated_at = Utc::now();

        // Update the mapping
        {
            let mut mappings = self.mappings.write().await;
            if let Some(mapping) = mappings.get_mut(id) {
                let old_short_url = mapping.short_url.clone();
                *mapping = updates;

                // Remind user to update DNS if short_url changed
                if old_short_url != mapping.short_url {
                    info!("Short URL changed. Update DNS: {} CNAME → <your-server>", mapping.short_url);
                }
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
