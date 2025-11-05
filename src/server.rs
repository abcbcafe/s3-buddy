#[allow(unused_imports)]
use axum::{
    extract::{Host, Path, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};
use uuid::Uuid;

use crate::manager::MappingManager;
use crate::s3::S3Client;
use crate::types::{CreateMappingRequest, ListMappingsResponse, Mapping, UpdateMappingRequest};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<MappingManager>,
    pub s3_client: Arc<S3Client>,
}

/// Create the HTTP API router
pub fn create_router(manager: Arc<MappingManager>, s3_client: Arc<S3Client>) -> Router {
    let state = AppState {
        manager,
        s3_client,
    };

    Router::new()
        .route("/health", get(health_check))
        .route("/mappings", get(list_mappings).post(create_mapping))
        .route(
            "/mappings/:id",
            get(get_mapping).put(update_mapping).delete(delete_mapping),
        )
        .route("/mappings/:id/pause", post(pause_mapping))
        .route("/mappings/:id/resume", post(resume_mapping))
        // Proxy route - must be last to catch all other requests
        .fallback(proxy_handler)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy"
    }))
}

/// List all mappings
async fn list_mappings(State(state): State<AppState>) -> impl IntoResponse {
    let mappings = state.manager.list_mappings().await;
    Json(ListMappingsResponse { mappings })
}

/// Get a specific mapping
async fn get_mapping(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Mapping>, StatusCode> {
    state
        .manager
        .get_mapping(&id)
        .await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// Create a new mapping
async fn create_mapping(
    State(state): State<AppState>,
    Json(req): Json<CreateMappingRequest>,
) -> Result<Json<Mapping>, (StatusCode, String)> {
    let mut mapping = Mapping::new(req.s3_url, req.short_url, req.hosted_zone_id);
    mapping.presign_duration_secs = req.presign_duration_secs;

    match state.manager.add_mapping(mapping.clone()).await {
        Ok(_) => Ok(Json(mapping)),
        Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
    }
}

/// Update an existing mapping
async fn update_mapping(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateMappingRequest>,
) -> Result<Json<Mapping>, (StatusCode, String)> {
    // Get existing mapping
    let mut mapping = state
        .manager
        .get_mapping(&id)
        .await
        .ok_or((StatusCode::NOT_FOUND, "Mapping not found".to_string()))?;

    // Apply updates
    if let Some(s3_url) = req.s3_url {
        mapping.s3_url = s3_url;
    }
    if let Some(short_url) = req.short_url {
        mapping.short_url = short_url;
    }
    if let Some(hosted_zone_id) = req.hosted_zone_id {
        mapping.hosted_zone_id = hosted_zone_id;
    }
    if let Some(presign_duration_secs) = req.presign_duration_secs {
        mapping.presign_duration_secs = presign_duration_secs;
    }

    match state.manager.update_mapping(&id, mapping.clone()).await {
        Ok(_) => Ok(Json(mapping)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Delete a mapping
async fn delete_mapping(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    match state.manager.delete_mapping(&id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err((StatusCode::NOT_FOUND, e.to_string())),
    }
}

/// Pause a mapping
async fn pause_mapping(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Mapping>, (StatusCode, String)> {
    match state.manager.pause_mapping(&id).await {
        Ok(_) => {
            let mapping = state
                .manager
                .get_mapping(&id)
                .await
                .ok_or((StatusCode::NOT_FOUND, "Mapping not found".to_string()))?;
            Ok(Json(mapping))
        }
        Err(e) => Err((StatusCode::NOT_FOUND, e.to_string())),
    }
}

/// Resume a paused mapping
async fn resume_mapping(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Mapping>, (StatusCode, String)> {
    match state.manager.resume_mapping(&id).await {
        Ok(_) => {
            let mapping = state
                .manager
                .get_mapping(&id)
                .await
                .ok_or((StatusCode::NOT_FOUND, "Mapping not found".to_string()))?;
            Ok(Json(mapping))
        }
        Err(e) => Err((StatusCode::NOT_FOUND, e.to_string())),
    }
}

/// Proxy handler - generates presigned URLs on-demand and redirects
async fn proxy_handler(
    State(state): State<AppState>,
    Host(hostname): Host,
    req: Request,
) -> Result<Response, (StatusCode, String)> {
    let request_path = req.uri().path();

    info!("Proxy request: host={}, path={}", hostname, request_path);

    // Find mapping by short_url (hostname)
    let mappings = state.manager.list_mappings().await;
    let mapping = mappings
        .iter()
        .find(|m| m.short_url == hostname)
        .ok_or_else(|| {
            warn!("No mapping found for hostname: {}", hostname);
            (
                StatusCode::NOT_FOUND,
                format!("No mapping configured for {}", hostname),
            )
        })?;

    // Check if mapping is active
    if mapping.status != crate::types::MappingStatus::Active {
        warn!("Mapping {} is not active (status: {:?})", mapping.id, mapping.status);
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            format!("Mapping is currently {}", mapping.status),
        ));
    }

    // Parse S3 base path
    let (bucket, base_key) = mapping.parse_s3_url().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to parse S3 URL: {}", e),
        )
    })?;

    // Combine base path with request path
    // Remove leading slash from request path
    let request_path = request_path.trim_start_matches('/');
    let full_key = if base_key.is_empty() {
        request_path.to_string()
    } else {
        // Ensure base_key doesn't end with slash to avoid double slashes
        let base = base_key.trim_end_matches('/');
        if request_path.is_empty() {
            base.to_string()
        } else {
            format!("{}/{}", base, request_path)
        }
    };

    info!("Generating presigned URL for s3://{}/{}", bucket, full_key);

    // Generate presigned URL
    let presigned_url = state
        .s3_client
        .generate_presigned_url(&bucket, &full_key, mapping.presign_duration())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to generate presigned URL: {}", e),
            )
        })?;

    info!("Redirecting to presigned URL");

    // Return 302 redirect
    Ok(Redirect::temporary(&presigned_url).into_response())
}
