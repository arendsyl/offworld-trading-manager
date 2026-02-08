use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use tracing::{debug, info, warn, instrument};

use crate::error::AppError;
use crate::models::{CreateSettlementRequest, Economy, Planet, PlanetStatus, Settlement};
use crate::state::AppState;

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

#[instrument(skip(state))]
async fn list_settlements_in_system(
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

#[instrument(skip(state))]
async fn get_settlement(
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

#[instrument(skip(state, payload))]
async fn create_or_update_settlement(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
    Json(payload): Json<CreateSettlementRequest>,
) -> Result<(StatusCode, Json<Settlement>), AppError> {
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
        population: payload.population.unwrap_or(0),
        economy: Economy::default(),
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

#[instrument(skip(state))]
async fn delete_settlement(
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
