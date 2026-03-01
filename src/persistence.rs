use sqlx::PgPool;
use tracing::info;

use crate::error::AppError;
use crate::market::MarketState;
use crate::models::{OrderStatus, PlanetStatus};
use crate::state::AppState;

/// Save a snapshot of the current game state to the database.
/// Acquires read locks in fixed order: galaxy → players → ships → projects → trade_requests → market
pub async fn save_snapshot(
    state: &AppState,
    pool: &PgPool,
    label: Option<&str>,
) -> Result<i64, AppError> {
    // Acquire read locks in fixed order
    let galaxy = state.galaxy.read().await;
    let players = state.players.read().await;
    let ships = state.ships.read().await;
    let projects = state.projects.read().await;
    let trade_requests = state.trade_requests.read().await;
    let market = state.market.read().await;

    // Serialize to JSON
    let galaxy_json = serde_json::to_value(&galaxy.systems)
        .map_err(|e| AppError::Internal(format!("failed to serialize galaxy: {e}")))?;
    let players_json = serde_json::to_value(&*players)
        .map_err(|e| AppError::Internal(format!("failed to serialize players: {e}")))?;
    let ships_json = serde_json::to_value(&*ships)
        .map_err(|e| AppError::Internal(format!("failed to serialize ships: {e}")))?;
    let projects_json = serde_json::to_value(&*projects)
        .map_err(|e| AppError::Internal(format!("failed to serialize projects: {e}")))?;
    let trade_requests_json = serde_json::to_value(&*trade_requests)
        .map_err(|e| AppError::Internal(format!("failed to serialize trade_requests: {e}")))?;
    let orders_json = serde_json::to_value(&market.orders)
        .map_err(|e| AppError::Internal(format!("failed to serialize orders: {e}")))?;
    let last_prices_json = serde_json::to_value(&market.last_prices)
        .map_err(|e| AppError::Internal(format!("failed to serialize last_prices: {e}")))?;

    // Drop locks before DB call
    drop(galaxy);
    drop(players);
    drop(ships);
    drop(projects);
    drop(trade_requests);
    drop(market);

    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO game_snapshots (label, galaxy, players, ships, projects, trade_requests, orders, last_prices)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id
        "#,
    )
    .bind(label)
    .bind(&galaxy_json)
    .bind(&players_json)
    .bind(&ships_json)
    .bind(&projects_json)
    .bind(&trade_requests_json)
    .bind(&orders_json)
    .bind(&last_prices_json)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Internal(format!("failed to save snapshot: {e}")))?;

    info!(snapshot_id = row.0, "Game snapshot saved");
    Ok(row.0)
}

/// Load a snapshot from the database and replace the current game state.
/// If snapshot_id is None, loads the most recent snapshot.
/// Acquires write locks in fixed order: galaxy → players → ships → projects → trade_requests → market
pub async fn load_snapshot(
    state: &AppState,
    pool: &PgPool,
    snapshot_id: Option<i64>,
) -> Result<i64, AppError> {
    let row: (i64, serde_json::Value, serde_json::Value, serde_json::Value, serde_json::Value, serde_json::Value, serde_json::Value, serde_json::Value) = match snapshot_id {
        Some(id) => {
            sqlx::query_as(
                "SELECT id, galaxy, players, ships, projects, trade_requests, orders, last_prices FROM game_snapshots WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AppError::Internal(format!("failed to query snapshot: {e}")))?
            .ok_or_else(|| AppError::Internal(format!("snapshot not found: {}", id)))?
        }
        None => {
            sqlx::query_as(
                "SELECT id, galaxy, players, ships, projects, trade_requests, orders, last_prices FROM game_snapshots ORDER BY saved_at DESC LIMIT 1",
            )
            .fetch_optional(pool)
            .await
            .map_err(|e| AppError::Internal(format!("failed to query snapshot: {e}")))?
            .ok_or_else(|| AppError::Internal("no snapshots found".to_string()))?
        }
    };

    let (loaded_id, galaxy_json, players_json, ships_json, projects_json, trade_requests_json, orders_json, last_prices_json) = row;

    // Deserialize
    let mut systems: std::collections::HashMap<String, crate::models::System> =
        serde_json::from_value(galaxy_json)
            .map_err(|e| AppError::Internal(format!("failed to deserialize galaxy: {e}")))?;

    // Initialize space elevator cabins (they use Instant, so are skipped during serde)
    for system in systems.values_mut() {
        for planet in &mut system.planets {
            if let PlanetStatus::Connected { space_elevator, .. } = &mut planet.status {
                space_elevator.ensure_cabins_initialized();
            }
        }
    }

    let players: std::collections::HashMap<String, crate::models::Player> =
        serde_json::from_value(players_json)
            .map_err(|e| AppError::Internal(format!("failed to deserialize players: {e}")))?;

    let ships: std::collections::HashMap<uuid::Uuid, crate::models::Ship> =
        serde_json::from_value(ships_json)
            .map_err(|e| AppError::Internal(format!("failed to deserialize ships: {e}")))?;

    let projects: std::collections::HashMap<uuid::Uuid, crate::models::ConstructionProject> =
        serde_json::from_value(projects_json)
            .map_err(|e| AppError::Internal(format!("failed to deserialize projects: {e}")))?;

    let trade_requests: std::collections::HashMap<uuid::Uuid, crate::models::TradeRequest> =
        serde_json::from_value(trade_requests_json)
            .map_err(|e| AppError::Internal(format!("failed to deserialize trade_requests: {e}")))?;

    let orders: std::collections::HashMap<uuid::Uuid, crate::models::Order> =
        serde_json::from_value(orders_json)
            .map_err(|e| AppError::Internal(format!("failed to deserialize orders: {e}")))?;

    let last_prices: std::collections::HashMap<String, u64> =
        serde_json::from_value(last_prices_json)
            .map_err(|e| AppError::Internal(format!("failed to deserialize last_prices: {e}")))?;

    // Reconstruct order books from open/partially-filled orders
    let trade_channel_capacity = state.config.market.trade_channel_capacity;
    let mut market = MarketState::new(trade_channel_capacity);
    market.last_prices = last_prices;

    // First insert all orders, then add open ones to books
    for (id, order) in &orders {
        if matches!(order.status, OrderStatus::Open | OrderStatus::PartiallyFilled) {
            if let Some(price) = order.price {
                let book = market.books.entry(order.good_name.clone()).or_insert_with(crate::market::OrderBook::new);
                match order.side {
                    crate::models::OrderSide::Buy => {
                        book.bids.entry(price).or_default().push_back(*id);
                    }
                    crate::models::OrderSide::Sell => {
                        book.asks.entry(price).or_default().push_back(*id);
                    }
                }
            }
        }
    }
    market.orders = orders;

    // Acquire write locks in fixed order and replace state
    let mut galaxy_w = state.galaxy.write().await;
    let mut players_w = state.players.write().await;
    let mut ships_w = state.ships.write().await;
    let mut projects_w = state.projects.write().await;
    let mut trade_requests_w = state.trade_requests.write().await;
    let mut market_w = state.market.write().await;

    galaxy_w.systems = systems;
    galaxy_w.connections.clear(); // Connections are ephemeral
    *players_w = players;
    *ships_w = ships;
    *projects_w = projects;
    *trade_requests_w = trade_requests;
    *market_w = market;

    info!(snapshot_id = loaded_id, "Game snapshot loaded");
    Ok(loaded_id)
}
