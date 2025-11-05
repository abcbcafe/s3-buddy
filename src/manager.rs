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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_manager_add_and_get_mapping() {
        let manager = MappingManager::new();

        let mapping = Mapping::new(
            "s3://test-bucket/docs/".to_string(),
            "docs.example.com".to_string(),
        );

        let id = manager.add_mapping(mapping.clone()).await.unwrap();
        let retrieved = manager.get_mapping(&id).await;

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.s3_url, "s3://test-bucket/docs/");
        assert_eq!(retrieved.short_url, "docs.example.com");
        assert_eq!(retrieved.status, MappingStatus::Active);
    }

    #[tokio::test]
    async fn test_manager_list_mappings() {
        let manager = MappingManager::new();

        let mapping1 = Mapping::new(
            "s3://bucket1/".to_string(),
            "files1.example.com".to_string(),
        );
        let mapping2 = Mapping::new(
            "s3://bucket2/".to_string(),
            "files2.example.com".to_string(),
        );

        manager.add_mapping(mapping1).await.unwrap();
        manager.add_mapping(mapping2).await.unwrap();

        let mappings = manager.list_mappings().await;
        assert_eq!(mappings.len(), 2);
    }

    #[tokio::test]
    async fn test_manager_update_mapping() {
        let manager = MappingManager::new();

        let mapping = Mapping::new(
            "s3://test-bucket/".to_string(),
            "files.example.com".to_string(),
        );

        let id = manager.add_mapping(mapping).await.unwrap();

        // Update the mapping
        let mut updated = manager.get_mapping(&id).await.unwrap();
        updated.presign_duration_secs = 900;

        manager.update_mapping(&id, updated).await.unwrap();

        let retrieved = manager.get_mapping(&id).await.unwrap();
        assert_eq!(retrieved.presign_duration_secs, 900);
    }

    #[tokio::test]
    async fn test_manager_pause_resume() {
        let manager = MappingManager::new();

        let mapping = Mapping::new(
            "s3://test-bucket/".to_string(),
            "files.example.com".to_string(),
        );

        let id = manager.add_mapping(mapping).await.unwrap();

        // Pause
        manager.pause_mapping(&id).await.unwrap();
        let retrieved = manager.get_mapping(&id).await.unwrap();
        assert_eq!(retrieved.status, MappingStatus::Paused);

        // Resume
        manager.resume_mapping(&id).await.unwrap();
        let retrieved = manager.get_mapping(&id).await.unwrap();
        assert_eq!(retrieved.status, MappingStatus::Active);
    }

    #[tokio::test]
    async fn test_manager_delete_mapping() {
        let manager = MappingManager::new();

        let mapping = Mapping::new(
            "s3://test-bucket/".to_string(),
            "files.example.com".to_string(),
        );

        let id = manager.add_mapping(mapping).await.unwrap();
        assert!(manager.get_mapping(&id).await.is_some());

        manager.delete_mapping(&id).await.unwrap();
        assert!(manager.get_mapping(&id).await.is_none());
    }

    #[tokio::test]
    async fn test_manager_invalid_s3_url() {
        let manager = MappingManager::new();

        let mapping = Mapping::new(
            "https://example.com/file".to_string(),
            "files.example.com".to_string(),
        );

        let result = manager.add_mapping(mapping).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_manager_get_nonexistent() {
        let manager = MappingManager::new();
        let fake_id = uuid::Uuid::new_v4();
        assert!(manager.get_mapping(&fake_id).await.is_none());
    }

    #[tokio::test]
    async fn test_manager_update_nonexistent() {
        let manager = MappingManager::new();
        let fake_id = uuid::Uuid::new_v4();
        let mapping = Mapping::new(
            "s3://bucket/".to_string(),
            "files.example.com".to_string(),
        );

        let result = manager.update_mapping(&fake_id, mapping).await;
        assert!(result.is_err());
    }
}
