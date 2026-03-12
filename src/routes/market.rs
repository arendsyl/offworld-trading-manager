use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::instrument;
use uuid::Uuid;

use crate::auth::AuthenticatedPlayer;
use crate::error::{AppError, MarketError};
use crate::models::{
    Order, OrderBookSummary, OrderSide, OrderStatus, OrderType, PlaceOrderRequest,
    PlanetStatus, Ship, ShipStatus,
};
use crate::ship_lifecycle::{calculate_sol_to_planet_time, spawn_transit_to_origin};
use crate::state::AppState;
use crate::validation::validate_input;

pub fn player_market_router() -> Router<AppState> {
    Router::new()
        .route("/orders", post(place_order).get(list_orders))
        .route("/orders/{order_id}", get(get_order).delete(cancel_order))
        .route("/book/{good_name}", get(get_order_book))
        .route("/trades", get(trade_stream))
        .route("/prices", get(get_prices))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_millis() as u64
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct OrderQuery {
    pub status: Option<String>,
}

#[utoipa::path(
    post,
    path = "/market/orders",
    tag = "market",
    security(("api_key" = [])),
    request_body = PlaceOrderRequest,
    responses(
        (status = 201, description = "Order placed successfully", body = Order),
    ),
)]
#[instrument(skip(state, auth))]
pub async fn place_order(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Json(body): Json<PlaceOrderRequest>,
) -> Result<(StatusCode, Json<Order>), AppError> {
    validate_input(&body)?;
    // Validate limit orders have a price
    if body.order_type == OrderType::Limit && body.price.is_none() {
        return Err(MarketError::PriceRequired.into());
    }

    // Validate station exists and is connected
    {
        let galaxy = state.galaxy.read().await;
        let mut found = false;
        for system in galaxy.systems.values() {
            for planet in &system.planets {
                if planet.id == body.station_planet_id {
                    if let PlanetStatus::Connected { ref station, .. } = planet.status {
                        if station.owner_id != auth.0.id {
                            return Err(MarketError::StationNotFoundForOrder(
                                body.station_planet_id.clone(),
                            )
                            .into());
                        }
                        found = true;
                    } else {
                        return Err(MarketError::StationNotFoundForOrder(
                            body.station_planet_id.clone(),
                        )
                        .into());
                    }
                }
            }
        }
        if !found {
            return Err(MarketError::StationNotFoundForOrder(body.station_planet_id.clone()).into());
        }
    }

    // Reservation at order placement
    match body.side {
        OrderSide::Buy => {
            if body.order_type == OrderType::Limit {
                let cost = body.price.expect("invariant: limit order has price") as i64 * body.quantity as i64;
                let mut players = state.players.write().await;
                let player = players
                    .get_mut(&auth.0.id)
                    .ok_or_else(|| AppError::PlayerNotFound(auth.0.id.clone()))?;
                if player.credits < cost {
                    return Err(MarketError::InsufficientCredits {
                        needed: cost,
                        available: player.credits,
                    }
                    .into());
                }
                player.credits -= cost;
            }
            // Market buy: credits deducted per match (handled later)
        }
        OrderSide::Sell => {
            // Deduct goods from station inventory upfront
            let mut galaxy = state.galaxy.write().await;
            for system in galaxy.systems.values_mut() {
                for planet in &mut system.planets {
                    if planet.id == body.station_planet_id {
                        if let PlanetStatus::Connected {
                            ref mut station, ..
                        } = planet.status
                        {
                            let available =
                                station.inventory.get(&body.good_name).copied().unwrap_or(0);
                            if available < body.quantity {
                                return Err(MarketError::InsufficientInventory {
                                    good_name: body.good_name.clone(),
                                    requested: body.quantity,
                                    available,
                                }
                                .into());
                            }
                            let entry = station
                                .inventory
                                .entry(body.good_name.clone())
                                .or_insert(0);
                            *entry -= body.quantity;
                            if *entry == 0 {
                                station.inventory.remove(&body.good_name);
                            }
                        }
                    }
                }
            }
        }
    }

    let order = Order {
        id: Uuid::new_v4(),
        player_id: auth.0.id.clone(),
        good_name: body.good_name.clone(),
        side: body.side.clone(),
        order_type: body.order_type.clone(),
        price: body.price,
        quantity: body.quantity,
        filled_quantity: 0,
        status: OrderStatus::Open,
        station_planet_id: body.station_planet_id.clone(),
        created_at: now_ms(),
    };

    let order_id = order.id;

    // Run matching engine
    let trades = {
        let mut market = state.market.write().await;
        market.place_order(order)
    };

    // Return unfilled goods for market sell orders
    if body.side == OrderSide::Sell && body.order_type == OrderType::Market {
        let unfilled = {
            let market = state.market.read().await;
            market.orders.get(&order_id).map(|o| o.remaining()).unwrap_or(0)
        };
        if unfilled > 0 {
            let mut galaxy = state.galaxy.write().await;
            for system in galaxy.systems.values_mut() {
                for planet in &mut system.planets {
                    if planet.id == body.station_planet_id {
                        if let PlanetStatus::Connected { ref mut station, .. } = planet.status {
                            let entry = station.inventory.entry(body.good_name.clone()).or_insert(0);
                            *entry += unfilled;
                        }
                    }
                }
            }
        }
    }

    // Process trades: transfer credits and spawn ships
    for trade in &trades {
        // Transfer credits between players
        {
            let mut players = state.players.write().await;
            let total_cost = trade.price as i64 * trade.quantity as i64;

            // For market buy orders, deduct from buyer now
            if body.side == OrderSide::Buy && body.order_type == OrderType::Market {
                if let Some(buyer) = players.get_mut(&trade.buyer_id) {
                    buyer.credits -= total_cost;
                }
            }

            // Credit seller
            if let Some(seller) = players.get_mut(&trade.seller_id) {
                seller.credits += total_cost;
            }

            // For limit buy orders, refund price difference if trade price < order price
            if body.side == OrderSide::Buy && body.order_type == OrderType::Limit {
                if let Some(order_price) = body.price {
                    let refund = (order_price as i64 - trade.price as i64) * trade.quantity as i64;
                    if refund > 0 {
                        if let Some(buyer) = players.get_mut(&trade.buyer_id) {
                            buyer.credits += refund;
                        }
                    }
                }
            }
        }

        // Spawn ship from Sol to seller's station (two-leg lifecycle)
        let mut cargo = HashMap::new();
        cargo.insert(trade.good_name.clone(), trade.quantity);

        // Calculate Sol → seller travel time and callback URL before building Ship
        let (transit_secs, seller_callback_url) = {
            let galaxy = state.galaxy.read().await;
            let seller_info = galaxy.find_planet_info(&trade.seller_station);

            let transit = match seller_info {
                Some((_, ref coords, au, _)) => {
                    calculate_sol_to_planet_time(coords, au, &state.config.trucking)
                }
                None => 0.0,
            };

            let seller_owner = seller_info
                .map(|(_, _, _, owner)| owner)
                .unwrap_or_default();

            let players = state.players.read().await;
            let callback = players
                .get(&seller_owner)
                .map(|p| p.callback_url.clone())
                .unwrap_or_default();

            (transit, callback)
        };

        let now = now_ms();
        let ship = Ship {
            id: Uuid::new_v4(),
            owner_id: trade.seller_id.clone(),
            origin_planet_id: trade.seller_station.clone(),
            destination_planet_id: trade.buyer_station.clone(),
            cargo,
            status: ShipStatus::InTransitToOrigin,
            trade_id: Some(trade.id),
            trucking_id: None,
            fee: None,
            created_at: now,
            arrival_at: None,
            operation_complete_at: None,
            estimated_arrival_at: Some(now + (transit_secs * 1000.0) as u64),
            callback_url: seller_callback_url.clone(),
        };

        let ship_id = ship.id;
        {
            let mut ships = state.ships.write().await;
            ships.insert(ship_id, ship);
        }

        if trade.seller_station != trade.buyer_station {
            spawn_transit_to_origin(
                state.ships.clone(),
                ship_id,
                transit_secs,
                seller_callback_url,
                state.config.ship.webhook_timeout_secs,
                state.http_client.clone(),
            );
        } else {
            // Same station: deliver immediately (skip storage check for same-station trades)
            {
                let mut ships = state.ships.write().await;
                if let Some(ship) = ships.get_mut(&ship_id) {
                    ship.status = ShipStatus::Complete;
                }
            }
            {
                let mut galaxy = state.galaxy.write().await;
                for system in galaxy.systems.values_mut() {
                    for planet in &mut system.planets {
                        if planet.id == trade.buyer_station {
                            if let PlanetStatus::Connected { ref mut station, .. } = planet.status {
                                let entry = station.inventory.entry(trade.good_name.clone()).or_insert(0);
                                *entry += trade.quantity;
                            }
                        }
                    }
                }
            }
        }
    }

    let market = state.market.read().await;
    let result_order = market
        .orders
        .get(&order_id)
        .cloned()
        .ok_or_else(|| MarketError::OrderNotFound(order_id.to_string()))?;

    Ok((StatusCode::CREATED, Json(result_order)))
}

#[utoipa::path(
    get,
    path = "/market/orders",
    tag = "market",
    security(("api_key" = [])),
    params(OrderQuery),
    responses(
        (status = 200, description = "List of player orders", body = Vec<Order>),
    ),
)]
#[instrument(skip(state, auth))]
pub async fn list_orders(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Query(query): Query<OrderQuery>,
) -> Json<Vec<Order>> {
    let market = state.market.read().await;
    let orders: Vec<Order> = market
        .orders
        .values()
        .filter(|o| {
            if o.player_id != auth.0.id {
                return false;
            }
            if let Some(ref status_str) = query.status {
                let status_json = format!("\"{}\"", status_str);
                let order_status_json = serde_json::to_string(&o.status).unwrap_or_default();
                if order_status_json != status_json {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();
    Json(orders)
}

#[utoipa::path(
    get,
    path = "/market/orders/{order_id}",
    tag = "market",
    security(("api_key" = [])),
    params(
        ("order_id" = Uuid, Path, description = "Order ID"),
    ),
    responses(
        (status = 200, description = "Order details", body = Order),
    ),
)]
#[instrument(skip(state, auth))]
pub async fn get_order(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(order_id): Path<Uuid>,
) -> Result<Json<Order>, AppError> {
    let market = state.market.read().await;
    let order = market
        .orders
        .get(&order_id)
        .cloned()
        .ok_or_else(|| MarketError::OrderNotFound(order_id.to_string()))?;
    if order.player_id != auth.0.id {
        return Err(AppError::Forbidden);
    }
    Ok(Json(order))
}

#[utoipa::path(
    delete,
    path = "/market/orders/{order_id}",
    tag = "market",
    security(("api_key" = [])),
    params(
        ("order_id" = Uuid, Path, description = "Order ID"),
    ),
    responses(
        (status = 200, description = "Order cancelled", body = Order),
    ),
)]
#[instrument(skip(state, auth))]
pub async fn cancel_order(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(order_id): Path<Uuid>,
) -> Result<Json<Order>, AppError> {
    let cancelled_order = {
        let mut market = state.market.write().await;

        // Verify ownership
        let order = market
            .orders
            .get(&order_id)
            .ok_or_else(|| MarketError::OrderNotFound(order_id.to_string()))?;

        if order.player_id != auth.0.id {
            return Err(AppError::Forbidden);
        }

        market
            .cancel_order(order_id)
            .ok_or(MarketError::OrderNotCancellable)?
    };

    // Return reserved credits/goods
    let remaining = cancelled_order.quantity - cancelled_order.filled_quantity;
    match cancelled_order.side {
        OrderSide::Buy => {
            if let Some(price) = cancelled_order.price {
                let refund = price as i64 * remaining as i64;
                let mut players = state.players.write().await;
                if let Some(player) = players.get_mut(&cancelled_order.player_id) {
                    player.credits += refund;
                }
            }
        }
        OrderSide::Sell => {
            let mut galaxy = state.galaxy.write().await;
            for system in galaxy.systems.values_mut() {
                for planet in &mut system.planets {
                    if planet.id == cancelled_order.station_planet_id {
                        if let PlanetStatus::Connected {
                            ref mut station, ..
                        } = planet.status
                        {
                            let entry = station
                                .inventory
                                .entry(cancelled_order.good_name.clone())
                                .or_insert(0);
                            *entry += remaining;
                        }
                    }
                }
            }
        }
    }

    Ok(Json(cancelled_order))
}

#[utoipa::path(
    get,
    path = "/market/book/{good_name}",
    tag = "market",
    security(("api_key" = [])),
    params(
        ("good_name" = String, Path, description = "Good name"),
    ),
    responses(
        (status = 200, description = "Order book summary", body = OrderBookSummary),
    ),
)]
#[instrument(skip(state))]
pub async fn get_order_book(
    State(state): State<AppState>,
    Path(good_name): Path<String>,
) -> Json<OrderBookSummary> {
    let market = state.market.read().await;
    let summary = market.get_order_book_summary(&good_name);
    Json(summary)
}

async fn trade_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = {
        let market = state.market.read().await;
        market.subscribe()
    };

    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(trade) => {
            let data = serde_json::to_string(&trade).unwrap_or_default();
            Some(Ok(Event::default().event("trade").data(data)))
        }
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
            Some(Ok(Event::default()
                .event("lagged")
                .data(format!("{{\"missed_events\":{}}}", n))))
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

#[utoipa::path(
    get,
    path = "/market/prices",
    tag = "market",
    security(("api_key" = [])),
    responses(
        (status = 200, description = "Last trade prices by good name", body = HashMap<String, u64>),
    ),
)]
#[instrument(skip(state))]
pub async fn get_prices(State(state): State<AppState>) -> Json<HashMap<String, u64>> {
    let market = state.market.read().await;
    Json(market.last_prices.clone())
}
