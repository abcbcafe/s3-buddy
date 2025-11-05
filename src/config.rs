use anyhow::{Context, Result};
use std::time::Duration;

/// Configuration for S3 Buddy
#[derive(Debug, Clone)]
pub struct Config {
    /// S3 URL (e.g., s3://bucket-name/path/to/object)
    pub s3_url: String,
    /// Short URL hostname (e.g., short.example.com)
    pub short_url: String,
    /// Route53 hosted zone ID
    pub hosted_zone_id: String,
    /// Presigned URL duration (default: 12 hours)
    pub presign_duration: Duration,
    /// Refresh interval (default: 11 hours to refresh before expiry)
    pub refresh_interval: Duration,
}

impl Config {
    pub fn new(s3_url: String, short_url: String, hosted_zone_id: String) -> Result<Self> {
        if !s3_url.starts_with("s3://") {
            anyhow::bail!("S3 URL must start with s3://");
        }

        Ok(Config {
            s3_url,
            short_url,
            hosted_zone_id,
            presign_duration: Duration::from_secs(12 * 60 * 60), // 12 hours
            refresh_interval: Duration::from_secs(11 * 60 * 60), // 11 hours
        })
    }

    /// Parse S3 URL into bucket and key
    pub fn parse_s3_url(&self) -> Result<(String, String)> {
        let url = self
            .s3_url
            .strip_prefix("s3://")
            .context("Invalid S3 URL format")?;

        let parts: Vec<&str> = url.splitn(2, '/').collect();
        if parts.len() != 2 {
            anyhow::bail!("S3 URL must include bucket and key: s3://bucket/key");
        }

        Ok((parts[0].to_string(), parts[1].to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_s3_url() {
        let config = Config::new(
            "s3://my-bucket/path/to/file.txt".to_string(),
            "short.example.com".to_string(),
            "Z1234567890ABC".to_string(), // This param still exists for legacy Config
        )
        .unwrap();

        let (bucket, key) = config.parse_s3_url().unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(key, "path/to/file.txt");
    }

    #[test]
    fn test_invalid_s3_url() {
        let result = Config::new(
            "https://example.com/file".to_string(),
            "short.example.com".to_string(),
            "Z1234567890ABC".to_string(),
        );
        assert!(result.is_err());
    }
}
