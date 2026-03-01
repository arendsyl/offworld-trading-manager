use axum::{
    extract::{Query, State},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;

use crate::error::AppError;
use crate::state::AppState;

pub fn admin_persistence_router() -> Router<AppState> {
    Router::new()
        .route("/save", post(save_snapshot))
        .route("/load", post(load_snapshot))
}

#[derive(Debug, Deserialize)]
pub struct SaveQuery {
    pub label: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoadQuery {
    pub snapshot_id: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SnapshotResponse {
    pub snapshot_id: i64,
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/admin/persistence/save",
    tag = "persistence",
    security(("bearer_auth" = [])),
    params(
        ("label" = Option<String>, Query, description = "Optional label for the snapshot"),
    ),
    responses(
        (status = 200, description = "Snapshot saved", body = SnapshotResponse),
        (status = 500, description = "Database not configured or save failed"),
    ),
)]
#[instrument(skip(state))]
pub async fn save_snapshot(
    State(state): State<AppState>,
    Query(query): Query<SaveQuery>,
) -> Result<Json<SnapshotResponse>, AppError> {
    let pool = state
        .db
        .as_ref()
        .ok_or_else(|| AppError::Internal("database not configured".to_string()))?;

    let snapshot_id =
        crate::persistence::save_snapshot(&state, pool, query.label.as_deref()).await?;

    Ok(Json(SnapshotResponse {
        snapshot_id,
        message: "snapshot saved successfully".to_string(),
    }))
}

#[utoipa::path(
    post,
    path = "/admin/persistence/load",
    tag = "persistence",
    security(("bearer_auth" = [])),
    params(
        ("snapshot_id" = Option<i64>, Query, description = "Snapshot ID to load (latest if omitted)"),
    ),
    responses(
        (status = 200, description = "Snapshot loaded", body = SnapshotResponse),
        (status = 500, description = "Database not configured or load failed"),
    ),
)]
#[instrument(skip(state))]
pub async fn load_snapshot(
    State(state): State<AppState>,
    Query(query): Query<LoadQuery>,
) -> Result<Json<SnapshotResponse>, AppError> {
    let pool = state
        .db
        .as_ref()
        .ok_or_else(|| AppError::Internal("database not configured".to_string()))?;

    let snapshot_id = crate::persistence::load_snapshot(&state, pool, query.snapshot_id).await?;

    Ok(Json(SnapshotResponse {
        snapshot_id,
        message: "snapshot loaded successfully".to_string(),
    }))
}
