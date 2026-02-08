use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use tracing::instrument;
use uuid::Uuid;

use crate::auth::AuthenticatedPlayer;
use crate::error::{AppError, ShipError};
use crate::models::{
    CreateShipRequest, DockRequest, PlanetStatus, Ship, ShipStatus, ShipWebhookPayload,
    UndockRequest,
};
use crate::ship_lifecycle::{send_ship_webhook, spawn_ship_transit};
use crate::state::AppState;

pub fn player_ships_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_ship).get(list_ships))
        .route("/{ship_id}", get(get_ship))
        .route("/{ship_id}/dock", put(dock_ship))
        .route("/{ship_id}/undock", put(undock_ship))
}

#[derive(Debug, Deserialize)]
struct ShipQuery {
    player_id: Option<String>,
    status: Option<String>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[instrument(skip(state, auth))]
async fn create_ship(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Json(body): Json<CreateShipRequest>,
) -> Result<(StatusCode, Json<Ship>), AppError> {
    if body.origin_planet_id == body.destination_planet_id {
        return Err(ShipError::SameStation.into());
    }

    // Validate and deduct cargo in a single write lock
    let (origin_dist, dest_dist, dest_owner_id) = {
        let mut galaxy = state.galaxy.write().await;

        let mut origin_distance = None;
        let mut dest_distance = None;
        let mut dest_owner = String::new();

        // First pass: validate both planets
        for system in galaxy.systems.values() {
            for planet in &system.planets {
                if planet.id == body.origin_planet_id {
                    if let PlanetStatus::Connected { ref station, .. } = planet.status {
                        if station.owner_id != auth.0.id {
                            return Err(ShipError::NotStationOwner.into());
                        }
                        // Validate cargo availability
                        for (good, &qty) in &body.cargo {
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
                        origin_distance = Some(planet.distance_ua);
                    } else {
                        return Err(AppError::NotConnected(body.origin_planet_id.clone()));
                    }
                }
                if planet.id == body.destination_planet_id {
                    if let PlanetStatus::Connected { ref station, .. } = planet.status {
                        dest_distance = Some(planet.distance_ua);
                        dest_owner = station.owner_id.clone();
                    } else {
                        return Err(AppError::NotConnected(body.destination_planet_id.clone()));
                    }
                }
            }
        }

        let o_dist = origin_distance
            .ok_or_else(|| AppError::PlanetNotFound(body.origin_planet_id.clone()))?;
        let d_dist = dest_distance
            .ok_or_else(|| AppError::PlanetNotFound(body.destination_planet_id.clone()))?;

        // Deduct cargo from origin station
        for system in galaxy.systems.values_mut() {
            for planet in &mut system.planets {
                if planet.id == body.origin_planet_id {
                    if let PlanetStatus::Connected { ref mut station, .. } = planet.status {
                        for (good, &qty) in &body.cargo {
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

        (o_dist, d_dist, dest_owner)
    };

    let transit_secs = (origin_dist - dest_dist).abs() * state.config.ship.au_to_seconds;

    let callback_url = {
        let players = state.players.read().await;
        players
            .get(&dest_owner_id)
            .map(|p| p.callback_url.clone())
            .unwrap_or_default()
    };

    let ship = Ship {
        id: Uuid::new_v4(),
        owner_id: auth.0.id.clone(),
        origin_planet_id: body.origin_planet_id,
        destination_planet_id: body.destination_planet_id,
        cargo: body.cargo,
        status: ShipStatus::InTransit,
        trade_id: None,
        created_at: now_ms(),
        arrival_at: None,
        operation_complete_at: None,
    };

    let ship_id = ship.id;
    let ship_clone = ship.clone();

    {
        let mut ships = state.ships.write().await;
        ships.insert(ship_id, ship_clone);
    }

    spawn_ship_transit(
        state.ships.clone(),
        ship_id,
        transit_secs,
        callback_url,
        state.config.ship.clone(),
        state.http_client.clone(),
    );

    Ok((StatusCode::CREATED, Json(ship)))
}

#[instrument(skip(state))]
async fn list_ships(
    State(state): State<AppState>,
    Query(query): Query<ShipQuery>,
) -> Json<Vec<Ship>> {
    let ships = state.ships.read().await;
    let result: Vec<Ship> = ships
        .values()
        .filter(|s| {
            if let Some(ref pid) = query.player_id {
                if s.owner_id != *pid {
                    return false;
                }
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

#[instrument(skip(state))]
async fn get_ship(
    State(state): State<AppState>,
    Path(ship_id): Path<Uuid>,
) -> Result<Json<Ship>, AppError> {
    // Polling drives state: check if operation_complete_at has passed
    let mut ships = state.ships.write().await;
    let ship = ships
        .get_mut(&ship_id)
        .ok_or_else(|| ShipError::ShipNotFound(ship_id.to_string()))?;

    let now = now_ms();
    if matches!(ship.status, ShipStatus::Loading | ShipStatus::Unloading) {
        if let Some(complete_at) = ship.operation_complete_at {
            if now >= complete_at {
                ship.status = ShipStatus::AwaitingUndockingAuth;
            }
        }
    }

    Ok(Json(ship.clone()))
}

#[instrument(skip(state, auth))]
async fn dock_ship(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(ship_id): Path<Uuid>,
    Json(body): Json<DockRequest>,
) -> Result<Json<Ship>, AppError> {
    if !body.authorized {
        return Err(ShipError::InvalidShipState.into());
    }

    // Verify auth player owns the destination station
    {
        let ships = state.ships.read().await;
        let ship = ships
            .get(&ship_id)
            .ok_or_else(|| ShipError::ShipNotFound(ship_id.to_string()))?;

        let galaxy = state.galaxy.read().await;
        let mut is_owner = false;
        for system in galaxy.systems.values() {
            for planet in &system.planets {
                if planet.id == ship.destination_planet_id {
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

    let (ship_result, callback_url) = {
        let mut ships = state.ships.write().await;
        let ship = ships
            .get_mut(&ship_id)
            .ok_or_else(|| ShipError::ShipNotFound(ship_id.to_string()))?;

        if ship.status != ShipStatus::AwaitingDockingAuth {
            return Err(ShipError::InvalidShipState.into());
        }

        ship.status = ShipStatus::Docked;

        // Calculate loading/unloading time
        let total_cargo: u64 = ship.cargo.values().sum();
        let operation_secs = total_cargo as f64 * state.config.ship.seconds_per_unit;
        let now = now_ms();
        let complete_at = now + (operation_secs * 1000.0) as u64;

        // Determine if loading or unloading (ships always unload at destination)
        ship.status = ShipStatus::Unloading;
        ship.operation_complete_at = Some(complete_at);

        let ship_clone = ship.clone();

        // Get callback URL for ship owner
        let players = state.players.read().await;
        let callback = players
            .get(&ship_clone.owner_id)
            .map(|p| p.callback_url.clone())
            .unwrap_or_default();

        (ship_clone, callback)
    };

    // Send ShipDocked webhook
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

#[instrument(skip(state, auth))]
async fn undock_ship(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(ship_id): Path<Uuid>,
    Json(body): Json<UndockRequest>,
) -> Result<Json<Ship>, AppError> {
    if !body.authorized {
        return Err(ShipError::InvalidShipState.into());
    }

    // Verify auth player owns the destination station
    let ship_snapshot = {
        let ships = state.ships.read().await;
        let ship = ships
            .get(&ship_id)
            .ok_or_else(|| ShipError::ShipNotFound(ship_id.to_string()))?;
        ship.clone()
    };

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

    // Check and transition state
    {
        let mut ships = state.ships.write().await;
        let ship = ships
            .get_mut(&ship_id)
            .ok_or_else(|| ShipError::ShipNotFound(ship_id.to_string()))?;

        // Also check if operation is complete via polling
        let now = now_ms();
        if matches!(ship.status, ShipStatus::Loading | ShipStatus::Unloading) {
            if let Some(complete_at) = ship.operation_complete_at {
                if now >= complete_at {
                    ship.status = ShipStatus::AwaitingUndockingAuth;
                }
            }
        }

        if ship.status != ShipStatus::AwaitingUndockingAuth {
            return Err(ShipError::InvalidShipState.into());
        }

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
        ships.get(&ship_id).cloned().unwrap()
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
