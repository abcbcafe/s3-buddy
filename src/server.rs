#[allow(unused_imports)]
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::manager::MappingManager;
use crate::types::{CreateMappingRequest, ListMappingsResponse, Mapping, UpdateMappingRequest};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<MappingManager>,
}

/// Create the HTTP API router
pub fn create_router(manager: Arc<MappingManager>) -> Router {
    let state = AppState { manager };

    Router::new()
        .route("/health", get(health_check))
        .route("/mappings", get(list_mappings).post(create_mapping))
        .route(
            "/mappings/:id",
            get(get_mapping).put(update_mapping).delete(delete_mapping),
        )
        .route("/mappings/:id/pause", post(pause_mapping))
        .route("/mappings/:id/resume", post(resume_mapping))
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
    mapping.refresh_interval_secs = req.refresh_interval_secs;

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
    if let Some(refresh_interval_secs) = req.refresh_interval_secs {
        mapping.refresh_interval_secs = refresh_interval_secs;
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
