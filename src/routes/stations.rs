use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use tracing::{debug, info, warn, instrument};

use crate::error::AppError;
use crate::models::{
    Cabin, CreateStationRequest, MassDriver, PlanetStatus, SpaceElevator, SpaceElevatorConfig,
    Station, Warehouse,
};
use crate::state::AppState;

pub fn admin_stations_router() -> Router<AppState> {
    Router::new().route(
        "/{system_name}/{planet_id}/station",
        get(get_station)
            .put(create_or_update_station)
            .delete(delete_station),
    )
}

pub fn player_stations_router() -> Router<AppState> {
    Router::new().route(
        "/{system_name}/{planet_id}/station",
        get(get_station),
    )
}

#[instrument(skip(state))]
async fn get_station(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
) -> Result<Json<Station>, AppError> {
    debug!("Getting station");
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
        PlanetStatus::Connected { station, .. } => {
            debug!(planet_id = %planet_id, station_name = %station.name, "Station found");
            Ok(Json(station.clone()))
        }
        PlanetStatus::Settled { .. } => {
            warn!(planet_id = %planet_id, "Station not found - planet only settled");
            Err(AppError::StationNotFound(planet_id))
        }
        PlanetStatus::Uninhabited => {
            warn!(planet_id = %planet_id, "Station not found - planet uninhabited");
            Err(AppError::SettlementNotFound(planet_id))
        }
    }
}

#[instrument(skip(state, payload))]
async fn create_or_update_station(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
    Json(payload): Json<CreateStationRequest>,
) -> Result<(StatusCode, Json<Station>), AppError> {
    debug!(station_name = %payload.name, "Creating or updating station");
    let default_channels = state.config.mass_driver.default_channels;
    let mut galaxy = state.galaxy.write().await;

    let system = galaxy
        .systems
        .get_mut(&system_name)
        .ok_or_else(|| AppError::SystemNotFound(system_name.clone()))?;

    let planet = system
        .planets
        .iter_mut()
        .find(|p| p.id == planet_id)
        .ok_or_else(|| AppError::PlanetNotFound(planet_id.clone()))?;

    let station = Station {
        name: payload.name,
        owner_id: payload.owner_id.clone(),
        inventory: Default::default(),
        mass_driver: None,
    };

    let (is_new, new_status) = match &planet.status {
        PlanetStatus::Uninhabited => {
            warn!(planet_id = %planet_id, "Cannot create station - settlement required");
            return Err(AppError::SettlementRequired(planet_id));
        }
        PlanetStatus::Settled { settlement } => {
            let config = SpaceElevatorConfig::default();
            let cabins = (0..config.cabin_count).map(Cabin::new).collect();
            let space_elevator = SpaceElevator {
                warehouse: Warehouse {
                    owner_id: payload.owner_id,
                    inventory: Default::default(),
                },
                config,
                cabins,
            };
            let mut new_station = station.clone();
            new_station.mass_driver = Some(MassDriver::new(default_channels));
            (true, PlanetStatus::Connected {
                settlement: settlement.clone(),
                station: new_station,
                space_elevator,
            })
        }
        PlanetStatus::Connected { settlement, space_elevator, station: existing_station } => {
            let mut updated_station = station.clone();
            updated_station.mass_driver = existing_station.mass_driver.clone();
            (false, PlanetStatus::Connected {
                settlement: settlement.clone(),
                station: updated_station,
                space_elevator: space_elevator.clone(),
            })
        }
    };

    planet.status = new_status;

    let result_station = match &planet.status {
        PlanetStatus::Connected { station, .. } => station.clone(),
        _ => unreachable!(),
    };

    let status = if is_new {
        info!(planet_id = %planet_id, station_name = %result_station.name, "Station created with space elevator");
        StatusCode::CREATED
    } else {
        info!(planet_id = %planet_id, station_name = %result_station.name, "Station updated");
        StatusCode::OK
    };

    Ok((status, Json(result_station)))
}

#[instrument(skip(state))]
async fn delete_station(
    State(state): State<AppState>,
    Path((system_name, planet_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    debug!("Deleting station");
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
        PlanetStatus::Connected { settlement, .. } => {
            planet.status = PlanetStatus::Settled { settlement: settlement.clone() };
            info!(planet_id = %planet_id, "Station deleted successfully");
            Ok(StatusCode::NO_CONTENT)
        }
        PlanetStatus::Settled { .. } => {
            warn!(planet_id = %planet_id, "Station not found for deletion");
            Err(AppError::StationNotFound(planet_id))
        }
        PlanetStatus::Uninhabited => {
            warn!(planet_id = %planet_id, "Station not found - planet uninhabited");
            Err(AppError::SettlementNotFound(planet_id))
        }
    }
}
