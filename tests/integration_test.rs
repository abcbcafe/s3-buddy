use s3_buddy::Config;

#[test]
fn test_config_validation() {
    // Valid configuration
    let config = Config::new(
        "s3://test-bucket/test-file.txt".to_string(),
        "short.example.com".to_string(),
        "Z1234567890ABC".to_string(),
    );
    assert!(config.is_ok());

    // Invalid S3 URL (doesn't start with s3://)
    let config = Config::new(
        "https://example.com/file".to_string(),
        "short.example.com".to_string(),
        "Z1234567890ABC".to_string(),
    );
    assert!(config.is_err());
}

#[test]
fn test_s3_url_parsing() {
    let config = Config::new(
        "s3://my-bucket/path/to/my/file.txt".to_string(),
        "short.example.com".to_string(),
        "Z1234567890ABC".to_string(),
    )
    .unwrap();

    let (bucket, key) = config.parse_s3_url().unwrap();
    assert_eq!(bucket, "my-bucket");
    assert_eq!(key, "path/to/my/file.txt");
}

#[test]
fn test_config_defaults() {
    let config = Config::new(
        "s3://bucket/key".to_string(),
        "short.example.com".to_string(),
        "Z1234567890ABC".to_string(),
    )
    .unwrap();

    // Default presign duration should be 12 hours
    assert_eq!(config.presign_duration.as_secs(), 12 * 60 * 60);

    // Default refresh interval should be 11 hours
    assert_eq!(config.refresh_interval.as_secs(), 11 * 60 * 60);
}
