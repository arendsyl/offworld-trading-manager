use std::collections::HashMap;
use std::sync::Arc;

use s3::creds::Credentials;
use s3::{Bucket, Region};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use crate::config::S3Config;
use crate::construction_lifecycle::{spawn_construction_project, spawn_upgrade_project};
use crate::error::AppError;
use crate::market::MarketState;
use crate::models::{
    ConstructionProject, Order, OrderSide, OrderStatus, PlanetStatus, Player, ProjectStatus,
    ProjectType, Ship, ShipStatus, System, TradeRequest, TradeRequestStatus,
};
use crate::ship_lifecycle::{spawn_ship_transit, spawn_transit_to_origin};
use crate::state::AppState;
use crate::trade_lifecycle::spawn_trade_request_loop;

#[derive(Serialize, Deserialize)]
pub struct GameSnapshot {
    pub saved_at: u64,
    pub galaxy_systems: HashMap<String, System>,
    pub players: HashMap<String, Player>,
    pub ships: HashMap<Uuid, Ship>,
    pub projects: HashMap<Uuid, ConstructionProject>,
    pub trade_requests: HashMap<Uuid, TradeRequest>,
    pub market_orders: HashMap<Uuid, Order>,
    pub market_last_prices: HashMap<String, u64>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_millis() as u64
}

pub fn build_s3_bucket(config: &S3Config) -> Result<Arc<Bucket>, AppError> {
    let bucket_name = config
        .bucket
        .as_deref()
        .ok_or_else(|| AppError::Internal("S3 bucket name not configured".to_string()))?;

    let region = if let Some(ref endpoint) = config.endpoint {
        Region::Custom {
            region: config.region.clone().unwrap_or_else(|| "us-east-1".into()),
            endpoint: endpoint.clone(),
        }
    } else {
        Region::Custom {
            region: config.region.clone().unwrap_or_else(|| "us-east-1".into()),
            endpoint: format!(
                "https://s3.{}.amazonaws.com",
                config.region.as_deref().unwrap_or("us-east-1")
            ),
        }
    };

    let credentials =
        if let (Some(key), Some(secret)) = (&config.access_key_id, &config.secret_access_key) {
            Credentials::new(Some(key), Some(secret), None, None, None)
                .map_err(|e| AppError::Internal(format!("failed to create S3 credentials: {e}")))?
        } else {
            Credentials::default()
                .map_err(|e| AppError::Internal(format!("failed to create S3 credentials: {e}")))?
        };

    let mut bucket = Bucket::new(bucket_name, region, credentials)
        .map_err(|e| AppError::Internal(format!("failed to create S3 bucket handle: {e}")))?;

    if config.endpoint.is_some() {
        bucket.set_path_style();
    }

    Ok(Arc::new(*bucket))
}

pub async fn save_snapshot(
    state: &AppState,
    bucket: &Bucket,
    save_name: &str,
) -> Result<(), AppError> {
    // Acquire read locks in fixed order
    let galaxy = state.galaxy.read().await;
    let players = state.players.read().await;
    let ships = state.ships.read().await;
    let projects = state.projects.read().await;
    let trade_requests = state.trade_requests.read().await;
    let market = state.market.read().await;

    let snapshot = GameSnapshot {
        saved_at: now_ms(),
        galaxy_systems: galaxy.systems.clone(),
        players: players.clone(),
        ships: ships.clone(),
        projects: projects.clone(),
        trade_requests: trade_requests.clone(),
        market_orders: market.orders.clone(),
        market_last_prices: market.last_prices.clone(),
    };

    // Drop all locks before S3 call
    drop(galaxy);
    drop(players);
    drop(ships);
    drop(projects);
    drop(trade_requests);
    drop(market);

    let bytes = rmp_serde::to_vec(&snapshot)
        .map_err(|e| AppError::Internal(format!("failed to serialize snapshot: {e}")))?;

    let key = format!("saves/{save_name}.msgpack");

    bucket
        .put_object(&key, &bytes)
        .await
        .map_err(|e| AppError::Internal(format!("failed to upload snapshot to S3: {e}")))?;

    info!(key = %key, "Game snapshot saved to S3");
    Ok(())
}

pub async fn load_snapshot(
    state: &AppState,
    bucket: &Bucket,
    save_name: &str,
) -> Result<(), AppError> {
    let key = format!("saves/{save_name}.msgpack");

    let response = bucket
        .get_object(&key)
        .await
        .map_err(|e| AppError::Internal(format!("failed to download snapshot from S3: {e}")))?;

    let bytes = response.bytes();

    let snapshot: GameSnapshot = rmp_serde::from_slice(bytes)
        .map_err(|e| AppError::Internal(format!("failed to deserialize snapshot: {e}")))?;

    // Post-process: ensure_cabins_initialized on all Connected planets
    let mut systems = snapshot.galaxy_systems;
    for system in systems.values_mut() {
        for planet in &mut system.planets {
            if let PlanetStatus::Connected { space_elevator, .. } = &mut planet.status {
                space_elevator.ensure_cabins_initialized();
            }
        }
    }

    // Reconstruct MarketState with order books
    let trade_channel_capacity = state.config.market.trade_channel_capacity;
    let mut market = MarketState::new(trade_channel_capacity);
    market.last_prices = snapshot.market_last_prices;

    for (id, order) in &snapshot.market_orders {
        if matches!(
            order.status,
            OrderStatus::Open | OrderStatus::PartiallyFilled
        ) {
            if let Some(price) = order.price {
                let book = market
                    .books
                    .entry(order.good_name.clone())
                    .or_insert_with(crate::market::OrderBook::new);
                match order.side {
                    OrderSide::Buy => {
                        book.bids.entry(price).or_default().push_back(*id);
                    }
                    OrderSide::Sell => {
                        book.asks.entry(price).or_default().push_back(*id);
                    }
                }
            }
        }
    }
    market.orders = snapshot.market_orders;

    // Acquire write locks in fixed order and replace state
    let mut galaxy_w = state.galaxy.write().await;
    let mut players_w = state.players.write().await;
    let mut ships_w = state.ships.write().await;
    let mut projects_w = state.projects.write().await;
    let mut trade_requests_w = state.trade_requests.write().await;
    let mut market_w = state.market.write().await;

    galaxy_w.systems = systems;
    galaxy_w.connections.clear();
    *players_w = snapshot.players;
    *ships_w = snapshot.ships;
    *projects_w = snapshot.projects;
    *trade_requests_w = snapshot.trade_requests;
    *market_w = market;

    info!(key = %key, "Game snapshot loaded from S3");
    Ok(())
}

pub async fn check_save_exists(bucket: &Bucket, save_name: &str) -> bool {
    let key = format!("saves/{save_name}.msgpack");
    bucket.head_object(&key).await.is_ok()
}

/// Recover in-flight background tasks after loading a snapshot.
pub async fn recover_in_flight_tasks(state: &AppState) {
    let now = now_ms();

    // Recover ships
    let ship_tasks: Vec<_> = {
        let ships = state.ships.read().await;
        ships
            .iter()
            .filter_map(|(&id, ship)| match ship.status {
                ShipStatus::InTransitToOrigin | ShipStatus::InTransit => {
                    let remaining_ms = ship.estimated_arrival_at.unwrap_or(0).saturating_sub(now);
                    let remaining_secs = remaining_ms as f64 / 1000.0;
                    Some((
                        id,
                        ship.status.clone(),
                        remaining_secs,
                        ship.callback_url.clone(),
                    ))
                }
                _ => None,
            })
            .collect()
    };

    for (id, status, remaining_secs, callback_url) in ship_tasks {
        match status {
            ShipStatus::InTransitToOrigin => {
                spawn_transit_to_origin(
                    state.ships.clone(),
                    id,
                    remaining_secs,
                    callback_url,
                    state.config.ship.webhook_timeout_secs,
                    state.http_client.clone(),
                );
                info!(ship_id = %id, remaining_secs, "Recovered InTransitToOrigin ship");
            }
            ShipStatus::InTransit => {
                spawn_ship_transit(
                    state.ships.clone(),
                    id,
                    remaining_secs,
                    callback_url,
                    state.config.ship.clone(),
                    state.http_client.clone(),
                );
                info!(ship_id = %id, remaining_secs, "Recovered InTransit ship");
            }
            _ => {}
        }
    }

    // Recover construction projects
    let project_tasks: Vec<_> = {
        let projects = state.projects.read().await;
        projects
            .iter()
            .filter_map(|(&id, project)| match project.status {
                ProjectStatus::InTransit => {
                    let transit_ends = project.transit_ends_at.unwrap_or(0);
                    let remaining_transit_ms = transit_ends.saturating_sub(now);
                    let remaining_transit_secs = remaining_transit_ms as f64 / 1000.0;
                    let build_start = transit_ends.max(now);
                    let remaining_build_ms = project.completion_at.saturating_sub(build_start);
                    let remaining_build_secs = remaining_build_ms as f64 / 1000.0;
                    Some((
                        id,
                        project.project_type.clone(),
                        project.status.clone(),
                        remaining_transit_secs,
                        remaining_build_secs,
                        project.callback_url.clone(),
                    ))
                }
                ProjectStatus::Building => {
                    let remaining_ms = project.completion_at.saturating_sub(now);
                    let remaining_secs = remaining_ms as f64 / 1000.0;
                    Some((
                        id,
                        project.project_type.clone(),
                        project.status.clone(),
                        0.0,
                        remaining_secs,
                        project.callback_url.clone(),
                    ))
                }
                ProjectStatus::Complete => None,
            })
            .collect()
    };

    for (id, project_type, status, remaining_transit, remaining_build, callback_url) in
        project_tasks
    {
        match (&project_type, &status) {
            (
                ProjectType::InstallStation | ProjectType::FoundSettlement,
                ProjectStatus::InTransit,
            ) => {
                spawn_construction_project(
                    state.projects.clone(),
                    state.galaxy.clone(),
                    state.config.clone(),
                    id,
                    remaining_transit,
                    remaining_build,
                    callback_url,
                    state.http_client.clone(),
                );
                info!(project_id = %id, "Recovered InTransit construction project");
            }
            (
                ProjectType::InstallStation | ProjectType::FoundSettlement,
                ProjectStatus::Building,
            ) => {
                spawn_construction_project(
                    state.projects.clone(),
                    state.galaxy.clone(),
                    state.config.clone(),
                    id,
                    0.0,
                    remaining_build,
                    callback_url,
                    state.http_client.clone(),
                );
                info!(project_id = %id, "Recovered Building construction project");
            }
            (_, ProjectStatus::Building) => {
                // Upgrade projects (1-phase)
                spawn_upgrade_project(
                    state.projects.clone(),
                    state.galaxy.clone(),
                    state.config.clone(),
                    id,
                    remaining_build,
                    callback_url,
                    state.http_client.clone(),
                );
                info!(project_id = %id, "Recovered Building upgrade project");
            }
            _ => {}
        }
    }

    // Recover trade requests
    let trade_request_ids: Vec<Uuid> = {
        let trade_requests = state.trade_requests.read().await;
        trade_requests
            .iter()
            .filter_map(|(&id, req)| {
                if req.status == TradeRequestStatus::Active {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    };

    for id in trade_request_ids {
        spawn_trade_request_loop(
            state.trade_requests.clone(),
            state.galaxy.clone(),
            state.players.clone(),
            state.config.clone(),
            id,
        );
        info!(request_id = %id, "Recovered active trade request");
    }
}
