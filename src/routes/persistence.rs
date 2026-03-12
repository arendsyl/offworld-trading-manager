use axum::{
    extract::State,
    routing::post,
    Json, Router,
};
use serde::Serialize;
use tracing::instrument;
use utoipa::ToSchema;

use crate::error::AppError;
use crate::state::AppState;

pub fn admin_persistence_router() -> Router<AppState> {
    Router::new()
        .route("/save", post(save_snapshot))
        .route("/load", post(load_snapshot))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PersistenceResponse {
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/admin/persistence/save",
    tag = "persistence",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Snapshot saved", body = PersistenceResponse),
        (status = 500, description = "S3 not configured or save failed"),
    ),
)]
#[instrument(skip(state))]
pub async fn save_snapshot(
    State(state): State<AppState>,
) -> Result<Json<PersistenceResponse>, AppError> {
    let bucket = state
        .s3
        .as_ref()
        .ok_or_else(|| AppError::Internal("S3 not configured".to_string()))?;
    let save_name = state
        .config
        .save_name
        .as_deref()
        .ok_or_else(|| AppError::Internal("save_name not configured".to_string()))?;

    crate::persistence::save_snapshot(&state, bucket, save_name).await?;

    Ok(Json(PersistenceResponse {
        message: "snapshot saved successfully".to_string(),
    }))
}

#[utoipa::path(
    post,
    path = "/admin/persistence/load",
    tag = "persistence",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Snapshot loaded", body = PersistenceResponse),
        (status = 500, description = "S3 not configured or load failed"),
    ),
)]
#[instrument(skip(state))]
pub async fn load_snapshot(
    State(state): State<AppState>,
) -> Result<Json<PersistenceResponse>, AppError> {
    let bucket = state
        .s3
        .as_ref()
        .ok_or_else(|| AppError::Internal("S3 not configured".to_string()))?;
    let save_name = state
        .config
        .save_name
        .as_deref()
        .ok_or_else(|| AppError::Internal("save_name not configured".to_string()))?;

    crate::persistence::load_snapshot(&state, bucket, save_name).await?;
    crate::persistence::recover_in_flight_tasks(&state).await;

    Ok(Json(PersistenceResponse {
        message: "snapshot loaded successfully".to_string(),
    }))
}
