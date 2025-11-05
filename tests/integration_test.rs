use s3_buddy::{Mapping, MappingManager, MappingStatus};

#[test]
fn test_mapping_creation() {
    let mapping = Mapping::new(
        "s3://test-bucket/path/".to_string(),
        "files.example.com".to_string(),
    );

    assert_eq!(mapping.s3_url, "s3://test-bucket/path/");
    assert_eq!(mapping.short_url, "files.example.com");
    assert_eq!(mapping.status, MappingStatus::Pending);
    assert_eq!(mapping.presign_duration_secs, 5 * 60); // Default 5 minutes
}

#[test]
fn test_mapping_s3_url_parsing() {
    let mapping = Mapping::new(
        "s3://my-bucket/documents/reports/".to_string(),
        "files.example.com".to_string(),
    );

    let (bucket, key) = mapping.parse_s3_url().unwrap();
    assert_eq!(bucket, "my-bucket");
    assert_eq!(key, "documents/reports/");
}

#[test]
fn test_mapping_s3_url_parsing_no_path() {
    let mapping = Mapping::new(
        "s3://my-bucket".to_string(),
        "files.example.com".to_string(),
    );

    let (bucket, key) = mapping.parse_s3_url().unwrap();
    assert_eq!(bucket, "my-bucket");
    assert_eq!(key, "");
}

#[test]
fn test_mapping_s3_url_parsing_trailing_slash() {
    let mapping = Mapping::new(
        "s3://my-bucket/docs/".to_string(),
        "files.example.com".to_string(),
    );

    let (bucket, key) = mapping.parse_s3_url().unwrap();
    assert_eq!(bucket, "my-bucket");
    assert_eq!(key, "docs/");
}

#[test]
fn test_mapping_s3_url_parsing_invalid() {
    let mapping = Mapping::new(
        "https://example.com/file".to_string(),
        "files.example.com".to_string(),
    );

    assert!(mapping.parse_s3_url().is_err());
}

#[tokio::test]
async fn test_mapping_manager_crud() {
    let manager = MappingManager::new();

    // Create mapping
    let mapping = Mapping::new(
        "s3://test-bucket/docs/".to_string(),
        "docs.example.com".to_string(),
    );
    let id = manager.add_mapping(mapping.clone()).await.unwrap();

    // Retrieve mapping
    let retrieved = manager.get_mapping(&id).await;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().short_url, "docs.example.com");

    // List mappings
    let mappings = manager.list_mappings().await;
    assert_eq!(mappings.len(), 1);

    // Update mapping
    let mut updated = manager.get_mapping(&id).await.unwrap();
    updated.presign_duration_secs = 600;
    manager.update_mapping(&id, updated).await.unwrap();

    let retrieved = manager.get_mapping(&id).await.unwrap();
    assert_eq!(retrieved.presign_duration_secs, 600);

    // Pause mapping
    manager.pause_mapping(&id).await.unwrap();
    let retrieved = manager.get_mapping(&id).await.unwrap();
    assert_eq!(retrieved.status, MappingStatus::Paused);

    // Resume mapping
    manager.resume_mapping(&id).await.unwrap();
    let retrieved = manager.get_mapping(&id).await.unwrap();
    assert_eq!(retrieved.status, MappingStatus::Active);

    // Delete mapping
    manager.delete_mapping(&id).await.unwrap();
    assert!(manager.get_mapping(&id).await.is_none());
}

#[tokio::test]
async fn test_mapping_validation() {
    let manager = MappingManager::new();

    // Invalid S3 URL should fail
    let mapping = Mapping::new(
        "https://example.com/file".to_string(),
        "files.example.com".to_string(),
    );

    let result = manager.add_mapping(mapping).await;
    assert!(result.is_err());
}
