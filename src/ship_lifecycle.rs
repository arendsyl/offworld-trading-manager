use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::{ShipConfig, TruckingConfig};
use crate::models::{Coordinates, Ship, ShipStatus, ShipWebhookPayload};

pub fn calculate_travel_time(
    from_coords: &Coordinates,
    from_au: f64,
    to_coords: &Coordinates,
    to_au: f64,
    same_system: bool,
    config: &TruckingConfig,
) -> f64 {
    if same_system {
        (from_au - to_au).abs() * config.au_to_seconds
    } else {
        let dx = from_coords.x - to_coords.x;
        let dy = from_coords.y - to_coords.y;
        let dz = from_coords.z - to_coords.z;
        let distance_ly = (dx * dx + dy * dy + dz * dz).sqrt();
        let jumps = (distance_ly / config.jump_range_ly).ceil();
        from_au * config.au_to_seconds + jumps * config.jump_time_secs + to_au * config.au_to_seconds
    }
}

pub fn calculate_sol_to_planet_time(
    target_coords: &Coordinates,
    target_au: f64,
    config: &TruckingConfig,
) -> f64 {
    let is_sol = target_coords.x == 0.0 && target_coords.y == 0.0 && target_coords.z == 0.0;
    if is_sol {
        target_au * config.au_to_seconds
    } else {
        let dx = target_coords.x;
        let dy = target_coords.y;
        let dz = target_coords.z;
        let distance_ly = (dx * dx + dy * dy + dz * dz).sqrt();
        let jumps = (distance_ly / config.jump_range_ly).ceil();
        jumps * config.jump_time_secs + target_au * config.au_to_seconds
    }
}

pub fn spawn_transit_to_origin(
    ships: Arc<RwLock<HashMap<Uuid, Ship>>>,
    ship_id: Uuid,
    transit_duration_secs: f64,
    callback_url: String,
    webhook_timeout_secs: u64,
    http_client: reqwest::Client,
) {
    tokio::spawn(async move {
        let duration = Duration::from_secs_f64(transit_duration_secs);
        tokio::time::sleep(duration).await;

        let webhook_payload = {
            let mut ships_lock = ships.write().await;
            if let Some(ship) = ships_lock.get_mut(&ship_id) {
                if ship.status != ShipStatus::InTransitToOrigin {
                    return;
                }
                ship.status = ShipStatus::AwaitingOriginDockingAuth;

                Some(ShipWebhookPayload::OriginDockingRequest {
                    ship_id: ship.id,
                    origin_planet_id: ship.origin_planet_id.clone(),
                    destination_planet_id: ship.destination_planet_id.clone(),
                    cargo: ship.cargo.clone(),
                })
            } else {
                None
            }
        };

        if let Some(payload) = webhook_payload {
            if !callback_url.is_empty() {
                let timeout = Duration::from_secs(webhook_timeout_secs);
                match http_client
                    .post(&callback_url)
                    .json(&payload)
                    .timeout(timeout)
                    .send()
                    .await
                {
                    Ok(resp) => {
                        info!(ship_id = %ship_id, status = %resp.status(), "Origin docking webhook sent");
                    }
                    Err(e) => {
                        warn!(ship_id = %ship_id, error = %e, "Failed to send origin docking webhook (non-fatal)");
                    }
                }
            }
        }
    });
}

pub fn spawn_ship_transit(
    ships: Arc<RwLock<HashMap<Uuid, Ship>>>,
    ship_id: Uuid,
    transit_duration_secs: f64,
    callback_url: String,
    ship_config: ShipConfig,
    http_client: reqwest::Client,
) {
    tokio::spawn(async move {
        // Sleep for transit duration
        let duration = Duration::from_secs_f64(transit_duration_secs);
        tokio::time::sleep(duration).await;

        // Transition to AwaitingDockingAuth
        let webhook_payload = {
            let mut ships_lock = ships.write().await;
            if let Some(ship) = ships_lock.get_mut(&ship_id) {
                if ship.status != ShipStatus::InTransit {
                    return;
                }
                ship.status = ShipStatus::AwaitingDockingAuth;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("system clock before UNIX epoch")
                    .as_millis() as u64;
                ship.arrival_at = Some(now);

                Some(ShipWebhookPayload::DockingRequest {
                    ship_id: ship.id,
                    origin_planet_id: ship.origin_planet_id.clone(),
                    cargo: ship.cargo.clone(),
                })
            } else {
                None
            }
        };

        // Send webhook (non-fatal on failure)
        if let Some(payload) = webhook_payload {
            if !callback_url.is_empty() {
                let timeout = Duration::from_secs(ship_config.webhook_timeout_secs);
                match http_client
                    .post(&callback_url)
                    .json(&payload)
                    .timeout(timeout)
                    .send()
                    .await
                {
                    Ok(resp) => {
                        info!(ship_id = %ship_id, status = %resp.status(), "Docking webhook sent");
                    }
                    Err(e) => {
                        warn!(ship_id = %ship_id, error = %e, "Failed to send docking webhook (non-fatal)");
                    }
                }
            }
        }
    });
}

pub async fn send_ship_webhook(
    http_client: &reqwest::Client,
    callback_url: &str,
    payload: &ShipWebhookPayload,
    timeout_secs: u64,
    ship_id: Uuid,
) {
    if callback_url.is_empty() {
        return;
    }
    let timeout = Duration::from_secs(timeout_secs);
    match http_client
        .post(callback_url)
        .json(payload)
        .timeout(timeout)
        .send()
        .await
    {
        Ok(resp) => {
            info!(ship_id = %ship_id, status = %resp.status(), "Ship webhook sent");
        }
        Err(e) => {
            warn!(ship_id = %ship_id, error = %e, "Failed to send ship webhook (non-fatal)");
        }
    }
}
