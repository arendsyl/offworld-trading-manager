use std::time::Duration;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use tracing::{debug, info, warn, error, instrument};

use crate::auth::AuthenticatedPlayer;
use crate::error::{AppError, ConstructionError};
use crate::models::{
    PlanetStatus, SpaceElevatorError, SpaceElevatorStatus, TransferDirection,
    TransferRequest, TransferResult,
};
use crate::state::AppState;
use crate::validation::validate_input;

pub fn space_elevator_router() -> Router<AppState> {
    Router::new()
        .route(
            "/{system_name}/{planet_id}/space-elevator",
            get(get_space_elevator),
        )
        .route(
            "/{system_name}/{planet_id}/space-elevator/transfer",
            post(transfer),
        )
}

#[utoipa::path(
    get,
    path = "/settlements/{system_name}/{planet_id}/space-elevator",
    tag = "space-elevator",
    security(("api_key" = [])),
    params(
        ("system_name" = String, Path, description = "System name"),
        ("planet_id" = String, Path, description = "Planet ID"),
    ),
    responses(
        (status = 200, description = "Space elevator status", body = SpaceElevatorStatus),
    ),
)]
#[instrument(skip(state, auth))]
pub async fn get_space_elevator(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path((system_name, planet_id)): Path<(String, String)>,
) -> Result<Json<SpaceElevatorStatus>, AppError> {
    debug!("Getting space elevator status");
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
        PlanetStatus::Connected { space_elevator, station, .. } => {
            if station.owner_id != auth.0.id {
                return Err(AppError::Forbidden);
            }
            debug!(planet_id = %planet_id, "Space elevator status retrieved");
            Ok(Json(space_elevator.status()))
        }
        PlanetStatus::Settled { .. } => {
            warn!(planet_id = %planet_id, "Space elevator not found - planet not connected");
            Err(AppError::NotConnected(planet_id))
        }
        PlanetStatus::Uninhabited => {
            warn!(planet_id = %planet_id, "Space elevator not found - planet uninhabited");
            Err(AppError::SettlementNotFound(planet_id))
        }
    }
}

/// Blocking call to transfer resources via the space elevator.
/// 
/// This call demonstrates blocking HTTP patterns:
/// - Validates total quantity against cabin capacity
/// - Validates stock availability and reserves goods from source
/// - Acquires an available cabin (rejects if none available)
/// - Blocks for the transfer duration
/// - May fail with exponential probability (cabin goes under repair, goods returned)
/// - Returns only after transfer completes or fails
/// 
/// Flow:
/// - ToSurface: Station inventory -> Warehouse inventory
/// - ToOrbit: Warehouse inventory -> Station inventory
#[utoipa::path(
    post,
    path = "/settlements/{system_name}/{planet_id}/space-elevator/transfer",
    tag = "space-elevator",
    security(("api_key" = [])),
    params(
        ("system_name" = String, Path, description = "System name"),
        ("planet_id" = String, Path, description = "Planet ID"),
    ),
    request_body = TransferRequest,
    responses(
        (status = 200, description = "Transfer result", body = TransferResult),
    ),
)]
#[instrument(skip(state, auth, request), fields(direction = ?request.direction, item_count = request.items.len()))]
pub async fn transfer(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path((system_name, planet_id)): Path<(String, String)>,
    Json(request): Json<TransferRequest>,
) -> Result<Json<TransferResult>, AppError> {
    validate_input(&request)?;
    debug!("Starting space elevator transfer");
    let items = request.items.clone();

    // Validate request has items
    if items.is_empty() {
        warn!("Transfer rejected - no items provided");
        return Err(SpaceElevatorError::EmptyTransfer.into());
    }

    // Calculate total quantity
    let total_quantity: u64 = items.iter().map(|i| i.quantity).sum();

    // Step 1: Validate stock and reserve goods, acquire cabin
    let (cabin_id, transfer_duration) = {
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

        match &mut planet.status {
            PlanetStatus::Connected { space_elevator, station, .. } => {
                // Check ownership
                if station.owner_id != auth.0.id {
                    return Err(AppError::Forbidden);
                }

                // Check cabin capacity
                if total_quantity > space_elevator.config.cabin_capacity {
                    warn!(
                        total = total_quantity,
                        capacity = space_elevator.config.cabin_capacity,
                        "Transfer rejected - exceeds cabin capacity"
                    );
                    return Err(SpaceElevatorError::ExceedsCapacity {
                        total: total_quantity,
                        capacity: space_elevator.config.cabin_capacity,
                    }.into());
                }

                // Check and reserve all items from source inventory
                match request.direction {
                    TransferDirection::ToSurface => {
                        // Station -> Warehouse: check station inventory for all items
                        for item in &items {
                            let available = station.inventory.get(&item.good_name).copied().unwrap_or(0);
                            if available < item.quantity {
                                warn!(
                                    good_name = %item.good_name,
                                    requested = item.quantity,
                                    available = available,
                                    "Transfer rejected - insufficient stock in station"
                                );
                                return Err(SpaceElevatorError::InsufficientStock {
                                    good_name: item.good_name.clone(),
                                    requested: item.quantity,
                                    available,
                                }.into());
                            }
                        }
                        // Deduct all items from station (reserve for transfer)
                        for item in &items {
                            let entry = station.inventory.get_mut(&item.good_name)
                                .ok_or_else(|| AppError::Internal(format!("inventory for {} disappeared after validation", item.good_name)))?;
                            *entry -= item.quantity;
                        }
                        debug!(direction = "ToSurface", "Reserved items from station inventory");
                    }
                    TransferDirection::ToOrbit => {
                        // Storage capacity check: will station have room?
                        let current: u64 = station.inventory.values().sum();
                        if current + total_quantity > station.max_storage {
                            return Err(ConstructionError::StorageFull {
                                current,
                                max: station.max_storage,
                                incoming: total_quantity,
                            }
                            .into());
                        }

                        // Warehouse -> Station: check warehouse inventory for all items
                        for item in &items {
                            let available = space_elevator.warehouse.inventory.get(&item.good_name).copied().unwrap_or(0);
                            if available < item.quantity {
                                warn!(
                                    good_name = %item.good_name,
                                    requested = item.quantity,
                                    available = available,
                                    "Transfer rejected - insufficient stock in warehouse"
                                );
                                return Err(SpaceElevatorError::InsufficientStock {
                                    good_name: item.good_name.clone(),
                                    requested: item.quantity,
                                    available,
                                }.into());
                            }
                        }
                        // Deduct all items from warehouse (reserve for transfer)
                        for item in &items {
                            let entry = space_elevator.warehouse.inventory.get_mut(&item.good_name)
                                .ok_or_else(|| AppError::Internal(format!("warehouse inventory for {} disappeared after validation", item.good_name)))?;
                            *entry -= item.quantity;
                        }
                        debug!(direction = "ToOrbit", "Reserved items from warehouse inventory");
                    }
                }

                // Acquire cabin
                let cabin_id = space_elevator.try_acquire_cabin()?;
                debug!(cabin_id = cabin_id, "Cabin acquired for transfer");
                (cabin_id, space_elevator.transfer_duration_secs())
            }
            PlanetStatus::Settled { .. } => {
                warn!(planet_id = %planet_id, "Transfer rejected - planet not connected");
                return Err(AppError::NotConnected(planet_id));
            }
            PlanetStatus::Uninhabited => {
                warn!(planet_id = %planet_id, "Transfer rejected - planet uninhabited");
                return Err(AppError::SettlementNotFound(planet_id));
            }
        }
    };

    // Step 2: Block for transfer duration (lock released during sleep)
    debug!(duration_secs = transfer_duration, "Transfer in progress");
    tokio::time::sleep(Duration::from_secs(transfer_duration)).await;

    // Step 3: Complete transfer (check failure and update inventories)
    let failed = {
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

        if let PlanetStatus::Connected { space_elevator, station, .. } = &mut planet.status {
            let failed = space_elevator.check_transfer_failure();
            space_elevator.complete_transfer(cabin_id, failed);

            if failed {
                error!(
                    cabin_id = cabin_id,
                    planet_id = %planet_id,
                    "Transfer failed - cabin malfunction, goods returned to source"
                );
                // Return all goods to source
                match request.direction {
                    TransferDirection::ToSurface => {
                        for item in &items {
                            *station.inventory.entry(item.good_name.clone()).or_insert(0) += item.quantity;
                        }
                    }
                    TransferDirection::ToOrbit => {
                        for item in &items {
                            *space_elevator.warehouse.inventory.entry(item.good_name.clone()).or_insert(0) += item.quantity;
                        }
                    }
                }
            } else {
                info!(
                    cabin_id = cabin_id,
                    planet_id = %planet_id,
                    total_quantity = total_quantity,
                    direction = ?request.direction,
                    "Transfer completed successfully"
                );
                // Move all goods to destination
                match request.direction {
                    TransferDirection::ToSurface => {
                        for item in &items {
                            *space_elevator.warehouse.inventory.entry(item.good_name.clone()).or_insert(0) += item.quantity;
                        }
                    }
                    TransferDirection::ToOrbit => {
                        for item in &items {
                            *station.inventory.entry(item.good_name.clone()).or_insert(0) += item.quantity;
                        }
                    }
                }
            }
            failed
        } else {
            false
        }
    };

    // Step 4: Return result
    Ok(Json(TransferResult {
        success: !failed,
        cabin_id,
        duration_secs: transfer_duration,
        items,
        total_quantity,
        failure_reason: if failed {
            Some("Cabin malfunction during transfer. Goods returned to source.".to_string())
        } else {
            None
        },
    }))
}
