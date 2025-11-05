use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

/// Represents a single S3 URL mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mapping {
    /// Unique identifier for this mapping
    pub id: Uuid,
    /// S3 URL (e.g., s3://bucket-name/path/to/object)
    pub s3_url: String,
    /// Short URL hostname (e.g., short.example.com)
    pub short_url: String,
    /// Route53 hosted zone ID
    pub hosted_zone_id: String,
    /// Current status of the mapping
    pub status: MappingStatus,
    /// Presigned URL duration in seconds (default: 12 hours)
    #[serde(default = "default_presign_duration")]
    pub presign_duration_secs: u64,
    /// Refresh interval in seconds (default: 11 hours)
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: u64,
    /// When this mapping was created
    pub created_at: DateTime<Utc>,
    /// When this mapping was last updated
    pub updated_at: DateTime<Utc>,
    /// Last successful refresh timestamp
    pub last_refresh: Option<DateTime<Utc>>,
    /// Next scheduled refresh timestamp
    pub next_refresh: Option<DateTime<Utc>>,
    /// Last error message if any
    pub last_error: Option<String>,
}

fn default_presign_duration() -> u64 {
    12 * 60 * 60 // 12 hours
}

fn default_refresh_interval() -> u64 {
    11 * 60 * 60 // 11 hours
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
            refresh_interval_secs: default_refresh_interval(),
            created_at: now,
            updated_at: now,
            last_refresh: None,
            next_refresh: None,
            last_error: None,
        }
    }

    pub fn presign_duration(&self) -> Duration {
        Duration::from_secs(self.presign_duration_secs)
    }

    pub fn refresh_interval(&self) -> Duration {
        Duration::from_secs(self.refresh_interval_secs)
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
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: u64,
}

/// Request to update an existing mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMappingRequest {
    pub s3_url: Option<String>,
    pub short_url: Option<String>,
    pub hosted_zone_id: Option<String>,
    pub presign_duration_secs: Option<u64>,
    pub refresh_interval_secs: Option<u64>,
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
