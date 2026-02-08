use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tracing::{debug, info, warn, instrument};

use crate::error::AppError;
use crate::models::{CreatePlanetRequest, Planet, PlanetStatus, PlanetType, UpdatePlanetRequest};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct PlanetFilter {
    pub planet_type: Option<String>,
}

pub fn admin_planets_router() -> Router<AppState> {
    Router::new()
        .route("/{system_name}/planets", post(create_planet).get(list_planets))
        .route("/{system_name}/planets/{planet_id}", get(get_planet).put(update_planet).delete(delete_planet))
        .route("/{system_name}/{planet_id}", get(get_planet).put(update_planet).delete(delete_planet))
}

pub fn player_planets_router() -> Router<AppState> {
    Router::new()
        .route("/{system_name}/planets", get(list_planets))
        .route("/{system_name}/planets/{planet_id}", get(get_planet))
        .route("/{system_name}/{planet_id}", get(get_planet))
}

fn generate_planet_id(star_name: &str, position: u32) -> String {
    format!("{}-{}", star_name, position)
}

#[instrument(skip(state, payload), fields(planet_name = %payload.name))]
async fn create_planet(
    State(state): State<AppState>,
    Path(system_name): Path<String>,
    Json(payload): Json<CreatePlanetRequest>,
) -> Result<(StatusCode, Json<Planet>), AppError> {
    debug!("Creating new planet");
    let mut state = state.galaxy.write().await;
    let system = state
        .systems
        .get_mut(&system_name)
        .ok_or_else(|| AppError::SystemNotFound(system_name.clone()))?;

    let planet_id = generate_planet_id(&system.name, payload.position);

    if system.planets.iter().any(|p| p.id == planet_id) {
        warn!(planet_id = %planet_id, "Planet already exists");
        return Err(AppError::PlanetAlreadyExists(planet_id));
    }

    let planet = Planet {
        id: planet_id,
        name: payload.name,
        position: payload.position,
        distance_ua: payload.distance_ua,
        planet_type: payload.planet_type,
        status: PlanetStatus::Uninhabited,
    };

    system.planets.push(planet.clone());

    info!(planet_id = %planet.id, system_name = %system_name, "Planet created successfully");
    Ok((StatusCode::CREATED, Json(planet)))
}

#[instrument(skip(state))]
async fn list_planets(
    State(state): State<AppState>,
    Path(system_name): Path<String>,
    Query(filter): Query<PlanetFilter>,
) -> Result<Json<Vec<Planet>>, AppError> {
    debug!(filter = ?filter, "Listing planets");
    let state = state.galaxy.read().await;
    let system = state
        .systems
        .get(&system_name)
        .ok_or_else(|| AppError::SystemNotFound(system_name.clone()))?;

    let planets: Vec<Planet> = system
        .planets
        .iter()
        .filter(|p| {
            if let Some(ref type_filter) = filter.planet_type {
                match (&p.planet_type, type_filter.as_str()) {
                    (PlanetType::Telluric { .. }, "telluric") => true,
                    (PlanetType::GasGiant { .. }, "gas_giant") => true,
                    _ => false,
                }
            } else {
                true
            }
        })
        .cloned()
        .collect();

    debug!(count = planets.len(), "Returning planets");
    Ok(Json(planets))
}

#[instrument(skip(state))]
async fn get_planet(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
) -> Result<Json<Planet>, AppError> {
    debug!("Getting planet");
    let state = state.galaxy.read().await;
    let system = state
        .systems
        .get(&system_name)
        .ok_or_else(|| AppError::SystemNotFound(system_name.clone()))?;

    let planet = system
        .planets
        .iter()
        .find(|p| p.id == planet_id)
        .cloned();

    match planet {
        Some(p) => {
            debug!(planet_id = %planet_id, "Planet found");
            Ok(Json(p))
        }
        None => {
            warn!(planet_id = %planet_id, "Planet not found");
            Err(AppError::PlanetNotFound(planet_id))
        }
    }
}

#[instrument(skip(state, payload))]
async fn update_planet(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
    Json(payload): Json<UpdatePlanetRequest>,
) -> Result<Json<Planet>, AppError> {
    debug!("Updating planet");
    let mut state = state.galaxy.write().await;
    let system = state
        .systems
        .get_mut(&system_name)
        .ok_or_else(|| AppError::SystemNotFound(system_name.clone()))?;

    let planet = system
        .planets
        .iter_mut()
        .find(|p| p.id == planet_id)
        .ok_or_else(|| AppError::PlanetNotFound(planet_id.clone()))?;

    if let Some(name) = payload.name {
        planet.name = name;
    }
    if let Some(distance_ua) = payload.distance_ua {
        planet.distance_ua = distance_ua;
    }
    if let Some(planet_type) = payload.planet_type {
        planet.planet_type = planet_type;
    }

    info!(planet_id = %planet_id, "Planet updated successfully");
    Ok(Json(planet.clone()))
}

#[instrument(skip(state))]
async fn delete_planet(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    debug!("Deleting planet");
    let mut state = state.galaxy.write().await;
    let system = state
        .systems
        .get_mut(&system_name)
        .ok_or_else(|| AppError::SystemNotFound(system_name.clone()))?;

    let initial_len = system.planets.len();
    system.planets.retain(|p| p.id != planet_id);

    if system.planets.len() < initial_len {
        info!(planet_id = %planet_id, "Planet deleted successfully");
        Ok(StatusCode::NO_CONTENT)
    } else {
        warn!(planet_id = %planet_id, "Planet not found for deletion");
        Err(AppError::PlanetNotFound(planet_id))
    }
}
