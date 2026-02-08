use std::sync::Arc;
use futures::StreamExt;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{info, warn, error, debug};
use pulsar::TokioExecutor;

use crate::config::AppConfig;
use crate::models::{
    ConnectionStatus, NotifyMessage, PacketItem, PlanetStatus, SendMessage,
};
use crate::pulsar::PulsarManager;
use crate::state::GalaxyState;

pub fn spawn_send_consumer(
    galaxy: Arc<RwLock<GalaxyState>>,
    pulsar: Arc<PulsarManager>,
    config: Arc<AppConfig>,
    player_id: String,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let consumer_result = pulsar.create_send_consumer(&player_id).await;
        let mut consumer: pulsar::consumer::Consumer<Vec<u8>, TokioExecutor> = match consumer_result {
            Ok(c) => c,
            Err(e) => {
                error!(error = %e, player_id = %player_id, "Failed to create send consumer");
                return;
            }
        };

        info!(player_id = %player_id, "Send consumer started");

        while let Some(msg_result) = consumer.next().await {
            let msg = match msg_result {
                Ok(m) => m,
                Err(e) => {
                    error!(error = %e, "Error receiving message");
                    continue;
                }
            };

            let payload = &msg.payload.data;
            let send_msg: SendMessage = match serde_json::from_slice(payload) {
                Ok(m) => m,
                Err(e) => {
                    warn!(error = %e, "Failed to deserialize send message");
                    let _ = consumer.ack(&msg).await;
                    continue;
                }
            };

            match send_msg {
                SendMessage::Packet { connection_id, items } => {
                    handle_packet(
                        &galaxy,
                        &pulsar,
                        &config,
                        connection_id,
                        items,
                    )
                    .await;
                }
            }

            let _ = consumer.ack(&msg).await;
        }

        info!(player_id = %player_id, "Send consumer stopped");
    })
}

async fn handle_packet(
    galaxy: &Arc<RwLock<GalaxyState>>,
    pulsar: &Arc<PulsarManager>,
    config: &Arc<AppConfig>,
    connection_id: uuid::Uuid,
    items: Vec<PacketItem>,
) {
    // Phase 1: Write lock — validate and deduct sender
    let (from_planet, to_planet, system_name, latency_secs, from_owner_id, to_owner_id) = {
        let mut state = galaxy.write().await;

        let connection = match state.connections.get(&connection_id) {
            Some(c) => c.clone(),
            None => {
                debug!(connection_id = %connection_id, "Connection not found for packet");
                return;
            }
        };

        // Resolve from_owner_id early so all paths can use it
        let from_owner_id = match state.resolve_planet_owner(&connection.system, &connection.from_planet) {
            Some(id) => id,
            None => return,
        };

        if connection.status != ConnectionStatus::Active {
            drop(state);
            send_packet_rejected(pulsar, &from_owner_id, connection.id, "Connection is not active").await;
            return;
        }

        // Validate packet size
        let total: u64 = items.iter().map(|i| i.quantity).sum();
        if total > config.mass_driver.max_packet_size {
            drop(state);
            send_packet_rejected(
                pulsar,
                &from_owner_id,
                connection.id,
                &format!(
                    "Packet too large: {} > {} max",
                    total, config.mass_driver.max_packet_size
                ),
            )
            .await;
            return;
        }

        // Find sender and validate/deduct inventory
        let system = match state.systems.get_mut(&connection.system) {
            Some(s) => s,
            None => return,
        };

        let from_distance = {
            let from = match system.planets.iter_mut().find(|p| p.id == connection.from_planet) {
                Some(p) => p,
                None => return,
            };

            match &mut from.status {
                PlanetStatus::Connected { station, .. } => {
                    // Validate inventory
                    for item in &items {
                        let available = station.inventory.get(&item.good_name).copied().unwrap_or(0);
                        if available < item.quantity {
                            let from_owner = from_owner_id.clone();
                            let conn_id = connection.id;
                            let reason = format!(
                                "Insufficient inventory: {} (need {}, have {})",
                                item.good_name, item.quantity, available
                            );
                            drop(state);
                            send_packet_rejected(
                                pulsar,
                                &from_owner,
                                conn_id,
                                &reason,
                            )
                            .await;
                            return;
                        }
                    }
                    // Deduct
                    for item in &items {
                        *station.inventory.get_mut(&item.good_name).unwrap() -= item.quantity;
                    }
                    from.distance_ua
                }
                _ => return,
            }
        };

        let (to_distance, to_owner_id) = {
            let to = match system.planets.iter().find(|p| p.id == connection.to_planet) {
                Some(p) => p,
                None => return,
            };
            let owner_id = match &to.status {
                PlanetStatus::Connected { station, .. } => station.owner_id.clone(),
                _ => return,
            };
            (to.distance_ua, owner_id)
        };

        let latency = (from_distance - to_distance).abs() * config.mass_driver.au_to_seconds;

        (
            connection.from_planet.clone(),
            connection.to_planet.clone(),
            connection.system.clone(),
            latency,
            from_owner_id,
            to_owner_id,
        )
    };

    // Notify sender that packet was sent
    pulsar
        .send_notification(
            &from_owner_id,
            &NotifyMessage::PacketSent {
                connection_id,
                items: items.clone(),
            },
        )
        .await;

    // Phase 2: No lock — simulate travel time
    let latency_duration = std::time::Duration::from_secs_f64(latency_secs);
    tokio::time::sleep(latency_duration).await;

    // Phase 3: Write lock — credit receiver (with storage check)
    let storage_rejected = {
        let mut state = galaxy.write().await;
        let system = match state.systems.get_mut(&system_name) {
            Some(s) => s,
            None => return,
        };

        let to = match system.planets.iter_mut().find(|p| p.id == to_planet) {
            Some(p) => p,
            None => return,
        };

        if let PlanetStatus::Connected { station, .. } = &mut to.status {
            let current: u64 = station.inventory.values().sum();
            let incoming: u64 = items.iter().map(|i| i.quantity).sum();
            if current + incoming > station.max_storage {
                true
            } else {
                for item in &items {
                    *station
                        .inventory
                        .entry(item.good_name.clone())
                        .or_insert(0) += item.quantity;
                }
                false
            }
        } else {
            false
        }
    };

    if storage_rejected {
        // Return goods to sender
        {
            let mut state = galaxy.write().await;
            let system = match state.systems.get_mut(&system_name) {
                Some(s) => s,
                None => return,
            };
            let from = match system.planets.iter_mut().find(|p| p.id == from_planet) {
                Some(p) => p,
                None => return,
            };
            if let PlanetStatus::Connected { station, .. } = &mut from.status {
                for item in &items {
                    *station
                        .inventory
                        .entry(item.good_name.clone())
                        .or_insert(0) += item.quantity;
                }
            }
        }
        send_packet_rejected(
            pulsar,
            &from_owner_id,
            connection_id,
            "Destination station storage full",
        )
        .await;
        return;
    }

    // Notify receiver
    pulsar
        .send_notification(
            &to_owner_id,
            &NotifyMessage::PacketReceived {
                connection_id,
                from_planet: from_planet.clone(),
                items,
            },
        )
        .await;
}

async fn send_packet_rejected(
    pulsar: &Arc<PulsarManager>,
    from_owner_id: &str,
    connection_id: uuid::Uuid,
    reason: &str,
) {
    pulsar
        .send_notification(
            from_owner_id,
            &NotifyMessage::PacketRejected {
                connection_id,
                reason: reason.to_string(),
            },
        )
        .await;
}
