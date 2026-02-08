use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::ShipConfig;
use crate::models::{Ship, ShipStatus, ShipWebhookPayload};

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
                    .unwrap()
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
