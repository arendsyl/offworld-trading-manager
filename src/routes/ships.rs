use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    routing::{get, put},
    Json, Router,
};
use serde::Deserialize;
use tracing::{instrument, warn};
use uuid::Uuid;

use crate::auth::AuthenticatedPlayer;
use crate::error::{AppError, ConstructionError, ShipError};
use crate::models::{
    DockRequest, PlanetStatus, Ship, ShipStatus, ShipWebhookPayload,
    UndockRequest,
};
use crate::ship_lifecycle::{
    calculate_travel_time, send_ship_webhook, spawn_ship_transit,
};
use crate::state::AppState;

fn count_docked_ships(ships: &HashMap<Uuid, Ship>, planet_id: &str) -> u32 {
    ships
        .values()
        .filter(|s| {
            (s.origin_planet_id == planet_id
                && matches!(
                    s.status,
                    ShipStatus::Loading | ShipStatus::AwaitingOriginUndockingAuth
                ))
                || (s.destination_planet_id == planet_id
                    && matches!(
                        s.status,
                        ShipStatus::Unloading | ShipStatus::AwaitingUndockingAuth
                    ))
        })
        .count() as u32
}

pub fn player_ships_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_ships))
        .route("/{ship_id}", get(get_ship))
        .route("/{ship_id}/dock", put(dock_ship))
        .route("/{ship_id}/undock", put(undock_ship))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ShipQuery {
    pub status: Option<String>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_millis() as u64
}

#[utoipa::path(
    get,
    path = "/ships",
    tag = "ships",
    params(ShipQuery),
    responses(
        (status = 200, description = "List of ships owned by the player", body = Vec<Ship>),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state, auth))]
pub async fn list_ships(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Query(query): Query<ShipQuery>,
) -> Json<Vec<Ship>> {
    let ships = state.ships.read().await;
    let result: Vec<Ship> = ships
        .values()
        .filter(|s| {
            if s.owner_id != auth.0.id {
                return false;
            }
            if let Some(ref status_str) = query.status {
                let status_json = format!("\"{}\"", status_str);
                let ship_status_json = serde_json::to_string(&s.status).unwrap_or_default();
                if ship_status_json != status_json {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();
    Json(result)
}

#[utoipa::path(
    get,
    path = "/ships/{ship_id}",
    tag = "ships",
    params(
        ("ship_id" = Uuid, Path, description = "Ship ID"),
    ),
    responses(
        (status = 200, description = "Ship details", body = Ship),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Ship not found"),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state, auth))]
pub async fn get_ship(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(ship_id): Path<Uuid>,
) -> Result<Json<Ship>, AppError> {
    let mut ships = state.ships.write().await;
    let ship = ships
        .get_mut(&ship_id)
        .ok_or_else(|| ShipError::ShipNotFound(ship_id.to_string()))?;

    // Allow access if caller owns the ship or owns the origin/destination station
    if ship.owner_id != auth.0.id {
        let galaxy = state.galaxy.read().await;
        let mut is_station_owner = false;
        for system in galaxy.systems.values() {
            for planet in &system.planets {
                if planet.id == ship.origin_planet_id || planet.id == ship.destination_planet_id {
                    if let PlanetStatus::Connected { ref station, .. } = planet.status {
                        if station.owner_id == auth.0.id {
                            is_station_owner = true;
                        }
                    }
                }
            }
        }
        if !is_station_owner {
            return Err(AppError::Forbidden);
        }
    }

    let now = now_ms();
    if let Some(complete_at) = ship.operation_complete_at {
        if now >= complete_at {
            match ship.status {
                ShipStatus::Loading => {
                    ship.status = ShipStatus::AwaitingOriginUndockingAuth;
                }
                ShipStatus::Unloading => {
                    ship.status = ShipStatus::AwaitingUndockingAuth;
                }
                _ => {
                    warn!(status = ?ship.status, "Unexpected status with completed operation timer");
                }
            }
        }
    }

    Ok(Json(ship.clone()))
}

#[utoipa::path(
    put,
    path = "/ships/{ship_id}/dock",
    tag = "ships",
    params(
        ("ship_id" = Uuid, Path, description = "Ship ID"),
    ),
    request_body = DockRequest,
    responses(
        (status = 200, description = "Ship docked", body = Ship),
        (status = 400, description = "Invalid ship state"),
        (status = 403, description = "Not station owner"),
        (status = 404, description = "Ship not found"),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state, auth))]
pub async fn dock_ship(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(ship_id): Path<Uuid>,
    Json(body): Json<DockRequest>,
) -> Result<Json<Ship>, AppError> {
    if !body.authorized {
        return Err(ShipError::InvalidShipState.into());
    }

    let ship_snapshot = {
        let ships = state.ships.read().await;
        ships
            .get(&ship_id)
            .ok_or_else(|| ShipError::ShipNotFound(ship_id.to_string()))?
            .clone()
    };

    match ship_snapshot.status {
        ShipStatus::AwaitingOriginDockingAuth => {
            // Origin docking: verify caller owns the origin station
            {
                let galaxy = state.galaxy.read().await;
                let mut is_owner = false;
                for system in galaxy.systems.values() {
                    for planet in &system.planets {
                        if planet.id == ship_snapshot.origin_planet_id {
                            if let PlanetStatus::Connected { ref station, .. } = planet.status {
                                if station.owner_id == auth.0.id {
                                    is_owner = true;
                                }
                            }
                        }
                    }
                }
                if !is_owner {
                    return Err(ShipError::NotStationOwner.into());
                }
            }

            // If trucking ship: validate & deduct cargo from origin station
            if ship_snapshot.trucking_id.is_some() {
                let mut galaxy = state.galaxy.write().await;
                for system in galaxy.systems.values_mut() {
                    for planet in &mut system.planets {
                        if planet.id == ship_snapshot.origin_planet_id {
                            if let PlanetStatus::Connected { ref mut station, .. } = planet.status {
                                // Validate cargo availability
                                for (good, &qty) in &ship_snapshot.cargo {
                                    let available = station.inventory.get(good).copied().unwrap_or(0);
                                    if available < qty {
                                        return Err(ShipError::InsufficientCargo {
                                            good_name: good.clone(),
                                            requested: qty,
                                            available,
                                        }
                                        .into());
                                    }
                                }
                                // Deduct cargo
                                for (good, &qty) in &ship_snapshot.cargo {
                                    let entry = station.inventory.entry(good.clone()).or_insert(0);
                                    *entry -= qty;
                                    if *entry == 0 {
                                        station.inventory.remove(good);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // If trade ship (trade_id.is_some()): cargo was already reserved at sell-order time, no deduction

            // Docking bay check
            {
                let galaxy = state.galaxy.read().await;
                let ships = state.ships.read().await;
                for system in galaxy.systems.values() {
                    for planet in &system.planets {
                        if planet.id == ship_snapshot.origin_planet_id {
                            if let PlanetStatus::Connected { ref station, .. } = planet.status {
                                let active_at_station =
                                    count_docked_ships(&ships, &ship_snapshot.origin_planet_id);
                                if active_at_station >= station.docking_bays {
                                    return Err(ConstructionError::NoDockingBayAvailable(
                                        ship_snapshot.origin_planet_id.clone(),
                                    )
                                    .into());
                                }
                            }
                        }
                    }
                }
            }

            // Set status to Loading
            let total_cargo: u64 = ship_snapshot.cargo.values().sum();
            let operation_secs = total_cargo as f64 * state.config.trucking.seconds_per_unit;
            let now = now_ms();
            let complete_at = now + (operation_secs * 1000.0) as u64;

            let ship_result = {
                let mut ships = state.ships.write().await;
                let ship = ships
                    .get_mut(&ship_id)
                    .ok_or_else(|| ShipError::ShipNotFound(ship_id.to_string()))?;
                ship.status = ShipStatus::Loading;
                ship.operation_complete_at = Some(complete_at);
                ship.clone()
            };

            // Send ShipDocked webhook
            let callback_url = {
                let players = state.players.read().await;
                players
                    .get(&ship_result.owner_id)
                    .map(|p| p.callback_url.clone())
                    .unwrap_or_default()
            };
            let payload = ShipWebhookPayload::ShipDocked {
                ship_id,
                status: "loading".to_string(),
            };
            send_ship_webhook(
                &state.http_client,
                &callback_url,
                &payload,
                state.config.ship.webhook_timeout_secs,
                ship_id,
            )
            .await;

            Ok(Json(ship_result))
        }
        ShipStatus::AwaitingDockingAuth => {
            // Destination docking: verify caller owns the destination station
            {
                let galaxy = state.galaxy.read().await;
                let mut is_owner = false;
                for system in galaxy.systems.values() {
                    for planet in &system.planets {
                        if planet.id == ship_snapshot.destination_planet_id {
                            if let PlanetStatus::Connected { ref station, .. } = planet.status {
                                if station.owner_id == auth.0.id {
                                    is_owner = true;
                                }
                            }
                        }
                    }
                }
                if !is_owner {
                    return Err(ShipError::NotStationOwner.into());
                }
            }

            // Docking bay check
            {
                let galaxy = state.galaxy.read().await;
                let ships = state.ships.read().await;
                for system in galaxy.systems.values() {
                    for planet in &system.planets {
                        if planet.id == ship_snapshot.destination_planet_id {
                            if let PlanetStatus::Connected { ref station, .. } = planet.status {
                                let active_at_station =
                                    count_docked_ships(&ships, &ship_snapshot.destination_planet_id);
                                if active_at_station >= station.docking_bays {
                                    return Err(ConstructionError::NoDockingBayAvailable(
                                        ship_snapshot.destination_planet_id.clone(),
                                    )
                                    .into());
                                }
                            }
                        }
                    }
                }
            }

            // Set status to Unloading
            let total_cargo: u64 = ship_snapshot.cargo.values().sum();
            let operation_secs = total_cargo as f64 * state.config.trucking.seconds_per_unit;
            let now = now_ms();
            let complete_at = now + (operation_secs * 1000.0) as u64;

            let ship_result = {
                let mut ships = state.ships.write().await;
                let ship = ships
                    .get_mut(&ship_id)
                    .ok_or_else(|| ShipError::ShipNotFound(ship_id.to_string()))?;

                if ship.status != ShipStatus::AwaitingDockingAuth {
                    return Err(ShipError::InvalidShipState.into());
                }

                ship.status = ShipStatus::Unloading;
                ship.operation_complete_at = Some(complete_at);
                ship.clone()
            };

            // Send ShipDocked webhook
            let callback_url = {
                let players = state.players.read().await;
                players
                    .get(&ship_result.owner_id)
                    .map(|p| p.callback_url.clone())
                    .unwrap_or_default()
            };
            let payload = ShipWebhookPayload::ShipDocked {
                ship_id,
                status: "unloading".to_string(),
            };
            send_ship_webhook(
                &state.http_client,
                &callback_url,
                &payload,
                state.config.ship.webhook_timeout_secs,
                ship_id,
            )
            .await;

            Ok(Json(ship_result))
        }
        _ => Err(ShipError::InvalidShipState.into()),
    }
}

#[utoipa::path(
    put,
    path = "/ships/{ship_id}/undock",
    tag = "ships",
    params(
        ("ship_id" = Uuid, Path, description = "Ship ID"),
    ),
    request_body = UndockRequest,
    responses(
        (status = 200, description = "Ship undocked", body = Ship),
        (status = 400, description = "Invalid ship state"),
        (status = 403, description = "Not station owner"),
        (status = 404, description = "Ship not found"),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state, auth))]
pub async fn undock_ship(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(ship_id): Path<Uuid>,
    Json(body): Json<UndockRequest>,
) -> Result<Json<Ship>, AppError> {
    if !body.authorized {
        return Err(ShipError::InvalidShipState.into());
    }

    let ship_snapshot = {
        let mut ships = state.ships.write().await;
        let ship = ships
            .get_mut(&ship_id)
            .ok_or_else(|| ShipError::ShipNotFound(ship_id.to_string()))?;

        // Polling: check if operation is complete
        let now = now_ms();
        if let Some(complete_at) = ship.operation_complete_at {
            if now >= complete_at {
                match ship.status {
                    ShipStatus::Loading => {
                        ship.status = ShipStatus::AwaitingOriginUndockingAuth;
                    }
                    ShipStatus::Unloading => {
                        ship.status = ShipStatus::AwaitingUndockingAuth;
                    }
                    _ => {
                        warn!(status = ?ship.status, "Unexpected status with completed operation timer");
                    }
                }
            }
        }

        ship.clone()
    };

    match ship_snapshot.status {
        ShipStatus::AwaitingOriginUndockingAuth => {
            // Origin undocking: verify caller owns the origin station
            {
                let galaxy = state.galaxy.read().await;
                let mut is_owner = false;
                for system in galaxy.systems.values() {
                    for planet in &system.planets {
                        if planet.id == ship_snapshot.origin_planet_id {
                            if let PlanetStatus::Connected { ref station, .. } = planet.status {
                                if station.owner_id == auth.0.id {
                                    is_owner = true;
                                }
                            }
                        }
                    }
                }
                if !is_owner {
                    return Err(ShipError::NotStationOwner.into());
                }
            }

            // Calculate travel time origin -> destination
            let (transit_secs, dest_callback_url) = {
                let galaxy = state.galaxy.read().await;
                let origin_info = galaxy.find_planet_info(&ship_snapshot.origin_planet_id);
                let dest_info = galaxy.find_planet_info(&ship_snapshot.destination_planet_id);

                let transit = match (origin_info, dest_info) {
                    (Some((origin_sys, origin_coords, origin_au, _)), Some((dest_sys, dest_coords, dest_au, _))) => {
                        let same_system = origin_sys == dest_sys;
                        calculate_travel_time(
                            &origin_coords,
                            origin_au,
                            &dest_coords,
                            dest_au,
                            same_system,
                            &state.config.trucking,
                        )
                    }
                    _ => 0.0,
                };

                let dest_owner = galaxy
                    .find_planet_info(&ship_snapshot.destination_planet_id)
                    .map(|(_, _, _, owner)| owner)
                    .unwrap_or_default();

                let players = state.players.read().await;
                let callback = players
                    .get(&dest_owner)
                    .map(|p| p.callback_url.clone())
                    .unwrap_or_default();

                (transit, callback)
            };

            // Set status to InTransit and spawn transit
            {
                let mut ships = state.ships.write().await;
                let ship = ships
                    .get_mut(&ship_id)
                    .ok_or_else(|| ShipError::ShipNotFound(ship_id.to_string()))?;
                ship.status = ShipStatus::InTransit;
                ship.estimated_arrival_at = Some(now_ms() + (transit_secs * 1000.0) as u64);
                ship.callback_url = dest_callback_url.clone();
            }

            spawn_ship_transit(
                state.ships.clone(),
                ship_id,
                transit_secs,
                dest_callback_url,
                state.config.ship.clone(),
                state.http_client.clone(),
            );

            let ship_result = {
                let ships = state.ships.read().await;
                ships.get(&ship_id).cloned()
                    .ok_or_else(|| AppError::Internal(format!("ship {ship_id} disappeared after transit")))?
            };

            Ok(Json(ship_result))
        }
        ShipStatus::AwaitingUndockingAuth => {
            // Destination undocking: verify caller owns the destination station
            {
                let galaxy = state.galaxy.read().await;
                let mut is_owner = false;
                for system in galaxy.systems.values() {
                    for planet in &system.planets {
                        if planet.id == ship_snapshot.destination_planet_id {
                            if let PlanetStatus::Connected { ref station, .. } = planet.status {
                                if station.owner_id == auth.0.id {
                                    is_owner = true;
                                }
                            }
                        }
                    }
                }
                if !is_owner {
                    return Err(ShipError::NotStationOwner.into());
                }
            }

            // Storage capacity check before transferring cargo
            {
                let galaxy = state.galaxy.read().await;
                for system in galaxy.systems.values() {
                    for planet in &system.planets {
                        if planet.id == ship_snapshot.destination_planet_id {
                            if let PlanetStatus::Connected { ref station, .. } = planet.status {
                                let current: u64 = station.inventory.values().sum();
                                let incoming: u64 = ship_snapshot.cargo.values().sum();
                                if current + incoming > station.max_storage {
                                    return Err(ConstructionError::StorageFull {
                                        current,
                                        max: station.max_storage,
                                        incoming,
                                    }
                                    .into());
                                }
                            }
                        }
                    }
                }
            }

            // Set status to Complete
            {
                let mut ships = state.ships.write().await;
                let ship = ships
                    .get_mut(&ship_id)
                    .ok_or_else(|| ShipError::ShipNotFound(ship_id.to_string()))?;
                ship.status = ShipStatus::Complete;
            }

            // Transfer cargo to destination station
            {
                let mut galaxy = state.galaxy.write().await;
                for system in galaxy.systems.values_mut() {
                    for planet in &mut system.planets {
                        if planet.id == ship_snapshot.destination_planet_id {
                            if let PlanetStatus::Connected {
                                ref mut station, ..
                            } = planet.status
                            {
                                for (good, &qty) in &ship_snapshot.cargo {
                                    let entry = station.inventory.entry(good.clone()).or_insert(0);
                                    *entry += qty;
                                }
                            }
                        }
                    }
                }
            }

            let ship_result = {
                let ships = state.ships.read().await;
                ships.get(&ship_id).cloned()
                    .ok_or_else(|| AppError::Internal(format!("ship {ship_id} disappeared after undock")))?
            };

            // Send ShipComplete webhook
            let players = state.players.read().await;
            let callback = players
                .get(&ship_snapshot.owner_id)
                .map(|p| p.callback_url.clone())
                .unwrap_or_default();
            drop(players);

            let payload = ShipWebhookPayload::ShipComplete { ship_id };
            send_ship_webhook(
                &state.http_client,
                &callback,
                &payload,
                state.config.ship.webhook_timeout_secs,
                ship_id,
            )
            .await;

            Ok(Json(ship_result))
        }
        _ => Err(ShipError::InvalidShipState.into()),
    }
}
