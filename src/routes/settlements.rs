use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use tracing::{debug, info, warn, instrument};

use crate::error::AppError;
use crate::economy::EconomyState;
use crate::models::{CreateSettlementRequest, Planet, PlanetStatus, Settlement};
use crate::state::AppState;
use crate::validation::validate_input;

pub fn admin_settlements_router() -> Router<AppState> {
    Router::new()
        .route("/{system_name}", get(list_settlements_in_system))
        .route(
            "/{system_name}/{planet_id}",
            get(get_settlement)
                .put(create_or_update_settlement)
                .delete(delete_settlement),
        )
}

pub fn player_settlements_router() -> Router<AppState> {
    Router::new()
        .route("/{system_name}", get(list_settlements_in_system))
        .route("/{system_name}/{planet_id}", get(get_settlement))
}

#[utoipa::path(
    get,
    path = "/settlements/{system_name}",
    tag = "settlements",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
    ),
    responses(
        (status = 200, description = "List of planets with settlements", body = Vec<Planet>),
        (status = 404, description = "System not found"),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state))]
pub async fn list_settlements_in_system(
    State(state): State<AppState>,
    Path(system_name): Path<String>,
) -> Result<Json<Vec<Planet>>, AppError> {
    debug!("Listing settlements in system");
    let state = state.galaxy.read().await;
    let system = state
        .systems
        .get(&system_name)
        .ok_or_else(|| AppError::SystemNotFound(system_name.clone()))?;

    let planets_with_settlements: Vec<Planet> = system
        .planets
        .iter()
        .filter(|p| !matches!(p.status, PlanetStatus::Uninhabited))
        .cloned()
        .collect();

    debug!(count = planets_with_settlements.len(), "Returning planets with settlements");
    Ok(Json(planets_with_settlements))
}

#[utoipa::path(
    get,
    path = "/settlements/{system_name}/{planet_id}",
    tag = "settlements",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
        ("planet_id" = String, Path, description = "ID of the planet"),
    ),
    responses(
        (status = 200, description = "Settlement details", body = Settlement),
        (status = 404, description = "System, planet, or settlement not found"),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state))]
pub async fn get_settlement(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
) -> Result<Json<Settlement>, AppError> {
    debug!("Getting settlement");
    let state = state.galaxy.read().await;
    let system = state
        .systems
        .get(&system_name)
        .ok_or_else(|| AppError::SystemNotFound(system_name.clone()))?;

    let planet = system
        .planets
        .iter()
        .find(|p| p.id == planet_id)
        .ok_or_else(|| AppError::PlanetNotFound(planet_id.clone()))?;

    match &planet.status {
        PlanetStatus::Settled { settlement } => {
            debug!(planet_id = %planet_id, "Settlement found");
            Ok(Json(settlement.clone()))
        }
        PlanetStatus::Connected { settlement, .. } => {
            debug!(planet_id = %planet_id, "Settlement found (connected)");
            Ok(Json(settlement.clone()))
        }
        PlanetStatus::Uninhabited => {
            warn!(planet_id = %planet_id, "Settlement not found - planet uninhabited");
            Err(AppError::SettlementNotFound(planet_id))
        }
    }
}

#[utoipa::path(
    put,
    path = "/admin/settlements/{system_name}/{planet_id}",
    tag = "settlements",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
        ("planet_id" = String, Path, description = "ID of the planet"),
    ),
    request_body = CreateSettlementRequest,
    responses(
        (status = 201, description = "Settlement created", body = Settlement),
        (status = 200, description = "Settlement updated", body = Settlement),
        (status = 404, description = "System or planet not found"),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state, payload))]
pub async fn create_or_update_settlement(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
    Json(payload): Json<CreateSettlementRequest>,
) -> Result<(StatusCode, Json<Settlement>), AppError> {
    validate_input(&payload)?;
    debug!(settlement_name = %payload.name, "Creating or updating settlement");
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

    let settlement = Settlement {
        name: payload.name,
        economy: EconomyState::default(),
        founding_goods: Default::default(),
    };

    let (is_new, new_status) = match &planet.status {
        PlanetStatus::Uninhabited => (true, PlanetStatus::Settled { settlement: settlement.clone() }),
        PlanetStatus::Settled { .. } => (false, PlanetStatus::Settled { settlement: settlement.clone() }),
        PlanetStatus::Connected { station, space_elevator, .. } => (false, PlanetStatus::Connected {
            settlement: settlement.clone(),
            station: station.clone(),
            space_elevator: space_elevator.clone(),
        }),
    };

    planet.status = new_status;

    let status = if is_new {
        info!(planet_id = %planet_id, settlement_name = %settlement.name, "Settlement created");
        StatusCode::CREATED
    } else {
        info!(planet_id = %planet_id, settlement_name = %settlement.name, "Settlement updated");
        StatusCode::OK
    };

    Ok((status, Json(settlement)))
}

#[utoipa::path(
    delete,
    path = "/admin/settlements/{system_name}/{planet_id}",
    tag = "settlements",
    params(
        ("system_name" = String, Path, description = "Name of the star system"),
        ("planet_id" = String, Path, description = "ID of the planet"),
    ),
    responses(
        (status = 204, description = "Settlement deleted"),
        (status = 404, description = "System, planet, or settlement not found"),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state))]
pub async fn delete_settlement(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    debug!("Deleting settlement");
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

    match &planet.status {
        PlanetStatus::Uninhabited => {
            warn!(planet_id = %planet_id, "Settlement not found for deletion");
            Err(AppError::SettlementNotFound(planet_id))
        }
        PlanetStatus::Settled { .. } | PlanetStatus::Connected { .. } => {
            planet.status = PlanetStatus::Uninhabited;
            info!(planet_id = %planet_id, "Settlement deleted successfully");
            Ok(StatusCode::NO_CONTENT)
        }
    }
}
