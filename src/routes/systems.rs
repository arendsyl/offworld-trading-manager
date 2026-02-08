use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tracing::{debug, info, warn, instrument};

use crate::models::{CreateSystemRequest, StarType, System, UpdateSystemRequest};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SystemFilter {
    pub star_type: Option<StarType>,
}

pub fn admin_systems_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_system).get(list_systems))
        .route("/{name}", get(get_system).put(update_system).delete(delete_system))
}

pub fn player_systems_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_systems))
        .route("/{name}", get(get_system))
}

#[instrument(skip(state, payload), fields(system_name = %payload.name))]
async fn create_system(
    State(state): State<AppState>,
    Json(payload): Json<CreateSystemRequest>,
) -> Result<(StatusCode, Json<System>), StatusCode> {
    debug!("Creating new system");
    let mut state = state.galaxy.write().await;

    if state.systems.contains_key(&payload.name) {
        warn!(system_name = %payload.name, "System already exists");
        return Err(StatusCode::CONFLICT);
    }

    let system = System {
        name: payload.name,
        coordinates: payload.coordinates,
        star_type: payload.star_type,
        planets: Vec::new(),
    };

    state.systems.insert(system.name.clone(), system.clone());

    info!(system_name = %system.name, "System created successfully");
    Ok((StatusCode::CREATED, Json(system)))
}

#[instrument(skip(state))]
async fn list_systems(
    State(state): State<AppState>,
    Query(filter): Query<SystemFilter>,
) -> Json<Vec<System>> {
    debug!(filter = ?filter, "Listing systems");
    let state = state.galaxy.read().await;
    let systems: Vec<System> = state
        .systems
        .values()
        .filter(|s| {
            if let Some(ref star_type) = filter.star_type {
                std::mem::discriminant(&s.star_type) == std::mem::discriminant(star_type)
            } else {
                true
            }
        })
        .cloned()
        .collect();
    debug!(count = systems.len(), "Returning systems");
    Json(systems)
}

#[instrument(skip(state))]
async fn get_system(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<System>, StatusCode> {
    debug!("Getting system");
    let state = state.galaxy.read().await;
    state
        .systems
        .get(&name)
        .cloned()
        .map(|s| {
            debug!(system_name = %name, "System found");
            Json(s)
        })
        .ok_or_else(|| {
            warn!(system_name = %name, "System not found");
            StatusCode::NOT_FOUND
        })
}

#[instrument(skip(state, payload))]
async fn update_system(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<UpdateSystemRequest>,
) -> Result<Json<System>, StatusCode> {
    debug!("Updating system");
    let mut state = state.galaxy.write().await;

    if let Some(new_name) = &payload.name {
        if new_name != &name && state.systems.contains_key(new_name) {
            warn!(system_name = %name, new_name = %new_name, "Cannot rename: target name already exists");
            return Err(StatusCode::CONFLICT);
        }
    }

    let system = state.systems.remove(&name).ok_or_else(|| {
        warn!(system_name = %name, "System not found for update");
        StatusCode::NOT_FOUND
    })?;
    let mut updated = system;

    if let Some(new_name) = payload.name {
        updated.name = new_name;
    }
    if let Some(coordinates) = payload.coordinates {
        updated.coordinates = coordinates;
    }
    if let Some(star_type) = payload.star_type {
        updated.star_type = star_type;
    }

    state.systems.insert(updated.name.clone(), updated.clone());
    info!(system_name = %updated.name, "System updated successfully");
    Ok(Json(updated))
}

#[instrument(skip(state))]
async fn delete_system(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    debug!("Deleting system");
    let mut state = state.galaxy.write().await;
    state
        .systems
        .remove(&name)
        .map(|_| {
            info!(system_name = %name, "System deleted successfully");
            StatusCode::NO_CONTENT
        })
        .ok_or_else(|| {
            warn!(system_name = %name, "System not found for deletion");
            StatusCode::NOT_FOUND
        })
}
