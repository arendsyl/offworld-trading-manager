use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tracing::{debug, info, warn, instrument};
use utoipa::IntoParams;

use crate::error::AppError;
use crate::models::{CreatePlanetRequest, Planet, PlanetStatus, PlanetType, UpdatePlanetRequest};
use crate::state::AppState;
use crate::validation::validate_input;

#[derive(Debug, Deserialize, IntoParams)]
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

#[utoipa::path(
    post,
    path = "/admin/systems/{system_name}/planets",
    tag = "planets",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
    ),
    request_body = CreatePlanetRequest,
    responses(
        (status = 201, description = "Planet created successfully", body = Planet),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state, payload), fields(planet_name = %payload.name))]
pub async fn create_planet(
    State(state): State<AppState>,
    Path(system_name): Path<String>,
    Json(payload): Json<CreatePlanetRequest>,
) -> Result<(StatusCode, Json<Planet>), AppError> {
    validate_input(&payload)?;
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
        resources: payload.resources.unwrap_or_default(),
        economy_config: payload.economy_config.unwrap_or_default(),
        planet_type: payload.planet_type,
        status: PlanetStatus::Uninhabited,
    };

    system.planets.push(planet.clone());

    info!(planet_id = %planet.id, system_name = %system_name, "Planet created successfully");
    Ok((StatusCode::CREATED, Json(planet)))
}

#[utoipa::path(
    get,
    path = "/systems/{system_name}/planets",
    tag = "planets",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
        PlanetFilter,
    ),
    responses(
        (status = 200, description = "List of planets", body = Vec<Planet>),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state))]
pub async fn list_planets(
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

#[utoipa::path(
    get,
    path = "/systems/{system_name}/planets/{planet_id}",
    tag = "planets",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
        ("planet_id" = String, Path, description = "ID of the planet"),
    ),
    responses(
        (status = 200, description = "Planet details", body = Planet),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state))]
pub async fn get_planet(
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

#[utoipa::path(
    put,
    path = "/admin/systems/{system_name}/planets/{planet_id}",
    tag = "planets",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
        ("planet_id" = String, Path, description = "ID of the planet"),
    ),
    request_body = UpdatePlanetRequest,
    responses(
        (status = 200, description = "Planet updated successfully", body = Planet),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state, payload))]
pub async fn update_planet(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
    Json(payload): Json<UpdatePlanetRequest>,
) -> Result<Json<Planet>, AppError> {
    validate_input(&payload)?;
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
    if let Some(economy_config) = payload.economy_config {
        planet.economy_config = economy_config;
    }

    info!(planet_id = %planet_id, "Planet updated successfully");
    Ok(Json(planet.clone()))
}

#[utoipa::path(
    delete,
    path = "/admin/systems/{system_name}/planets/{planet_id}",
    tag = "planets",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
        ("planet_id" = String, Path, description = "ID of the planet"),
    ),
    responses(
        (status = 204, description = "Planet deleted successfully"),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state))]
pub async fn delete_planet(
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
