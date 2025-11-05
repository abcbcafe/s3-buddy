use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

/// Represents a single S3 URL mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mapping {
    /// Unique identifier for this mapping
    pub id: Uuid,
    /// S3 URL base path (e.g., s3://bucket-name/base/path/)
    /// Request paths will be appended to this base
    pub s3_url: String,
    /// Short URL hostname (e.g., short.example.com)
    pub short_url: String,
    /// Route53 hosted zone ID
    pub hosted_zone_id: String,
    /// Current status of the mapping
    pub status: MappingStatus,
    /// Presigned URL duration in seconds (default: 5 minutes)
    /// URLs are generated on-demand, so shorter duration is more secure
    #[serde(default = "default_presign_duration")]
    pub presign_duration_secs: u64,
    /// When this mapping was created
    pub created_at: DateTime<Utc>,
    /// When this mapping was last updated
    pub updated_at: DateTime<Utc>,
    /// When DNS was last configured
    pub dns_configured_at: Option<DateTime<Utc>>,
    /// Last error message if any
    pub last_error: Option<String>,
}

fn default_presign_duration() -> u64 {
    5 * 60 // 5 minutes - shorter is more secure for on-demand generation
}

impl Mapping {
    pub fn new(s3_url: String, short_url: String, hosted_zone_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            s3_url,
            short_url,
            hosted_zone_id,
            status: MappingStatus::Pending,
            presign_duration_secs: default_presign_duration(),
            created_at: now,
            updated_at: now,
            dns_configured_at: None,
            last_error: None,
        }
    }

    pub fn presign_duration(&self) -> Duration {
        Duration::from_secs(self.presign_duration_secs)
    }

    /// Parse the S3 URL into bucket and base key path
    pub fn parse_s3_url(&self) -> anyhow::Result<(String, String)> {
        let url = self
            .s3_url
            .strip_prefix("s3://")
            .ok_or_else(|| anyhow::anyhow!("S3 URL must start with s3://"))?;

        let parts: Vec<&str> = url.splitn(2, '/').collect();
        let bucket = parts[0].to_string();
        let base_key = if parts.len() > 1 {
            parts[1].to_string()
        } else {
            String::new()
        };

        Ok((bucket, base_key))
    }
}

/// Status of a mapping
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MappingStatus {
    /// Waiting to be started
    Pending,
    /// Active and refreshing
    Active,
    /// Paused (not refreshing)
    Paused,
    /// Error state
    Error,
}

impl std::fmt::Display for MappingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MappingStatus::Pending => write!(f, "Pending"),
            MappingStatus::Active => write!(f, "Active"),
            MappingStatus::Paused => write!(f, "Paused"),
            MappingStatus::Error => write!(f, "Error"),
        }
    }
}

/// Request to create a new mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMappingRequest {
    pub s3_url: String,
    pub short_url: String,
    pub hosted_zone_id: String,
    #[serde(default = "default_presign_duration")]
    pub presign_duration_secs: u64,
}

/// Request to update an existing mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMappingRequest {
    pub s3_url: Option<String>,
    pub short_url: Option<String>,
    pub hosted_zone_id: Option<String>,
    pub presign_duration_secs: Option<u64>,
}

/// Response containing a list of mappings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListMappingsResponse {
    pub mappings: Vec<Mapping>,
}

/// Log entry for refresh operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshLog {
    pub mapping_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub message: String,
}
