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
    /// Short URL hostname (e.g., short.example.com) - used to match HTTP Host header
    /// Users must configure DNS manually: short_url CNAME → server hostname
    pub short_url: String,
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
    /// Last error message if any
    pub last_error: Option<String>,
}

fn default_presign_duration() -> u64 {
    5 * 60 // 5 minutes - shorter is more secure for on-demand generation
}

impl Mapping {
    pub fn new(s3_url: String, short_url: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            s3_url,
            short_url,
            status: MappingStatus::Pending,
            presign_duration_secs: default_presign_duration(),
            created_at: now,
            updated_at: now,
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
    #[serde(default = "default_presign_duration")]
    pub presign_duration_secs: u64,
}

/// Request to update an existing mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMappingRequest {
    pub s3_url: Option<String>,
    pub short_url: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mapping_new() {
        let mapping = Mapping::new(
            "s3://bucket/path/".to_string(),
            "files.example.com".to_string(),
        );

        assert_eq!(mapping.s3_url, "s3://bucket/path/");
        assert_eq!(mapping.short_url, "files.example.com");
        assert_eq!(mapping.status, MappingStatus::Pending);
        assert_eq!(mapping.presign_duration_secs, 300); // 5 minutes default
    }

    #[test]
    fn test_parse_s3_url_with_path() {
        let mapping = Mapping::new(
            "s3://my-bucket/documents/reports/".to_string(),
            "files.example.com".to_string(),
        );

        let (bucket, key) = mapping.parse_s3_url().unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(key, "documents/reports/");
    }

    #[test]
    fn test_parse_s3_url_no_trailing_slash() {
        let mapping = Mapping::new(
            "s3://my-bucket/docs".to_string(),
            "files.example.com".to_string(),
        );

        let (bucket, key) = mapping.parse_s3_url().unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(key, "docs");
    }

    #[test]
    fn test_parse_s3_url_bucket_only() {
        let mapping = Mapping::new(
            "s3://my-bucket".to_string(),
            "files.example.com".to_string(),
        );

        let (bucket, key) = mapping.parse_s3_url().unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(key, "");
    }

    #[test]
    fn test_parse_s3_url_invalid() {
        let mapping = Mapping::new(
            "https://example.com/file".to_string(),
            "files.example.com".to_string(),
        );

        assert!(mapping.parse_s3_url().is_err());
    }

    #[test]
    fn test_parse_s3_url_missing_prefix() {
        let mapping = Mapping::new(
            "bucket/key".to_string(),
            "files.example.com".to_string(),
        );

        assert!(mapping.parse_s3_url().is_err());
    }

    #[test]
    fn test_presign_duration_conversion() {
        let mut mapping = Mapping::new(
            "s3://bucket/key".to_string(),
            "files.example.com".to_string(),
        );

        mapping.presign_duration_secs = 600; // 10 minutes
        assert_eq!(mapping.presign_duration().as_secs(), 600);
    }

    #[test]
    fn test_mapping_status_display() {
        assert_eq!(MappingStatus::Pending.to_string(), "Pending");
        assert_eq!(MappingStatus::Active.to_string(), "Active");
        assert_eq!(MappingStatus::Paused.to_string(), "Paused");
        assert_eq!(MappingStatus::Error.to_string(), "Error");
    }
}
