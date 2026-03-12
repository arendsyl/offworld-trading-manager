use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use tracing::instrument;
use uuid::Uuid;

use crate::auth::AuthenticatedPlayer;
use crate::error::{AppError, TradeRequestError};
use crate::models::{
    CreateTradeRequestBody, PlanetStatus, TradeRequest, TradeRequestMode, TradeRequestStatus,
};
use crate::state::AppState;
use crate::trade_lifecycle::spawn_trade_request_loop;
use crate::validation::validate_input;

pub fn player_trade_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_trade_requests).post(create_trade_request))
        .route(
            "/{request_id}",
            get(get_trade_request).delete(cancel_trade_request),
        )
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_millis() as u64
}

#[utoipa::path(
    post,
    path = "/trade",
    tag = "trade",
    security(("api_key" = [])),
    request_body = CreateTradeRequestBody,
    responses(
        (status = 201, description = "Trade request created", body = TradeRequest),
    ),
)]
#[instrument(skip(state, auth))]
pub async fn create_trade_request(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Json(body): Json<CreateTradeRequestBody>,
) -> Result<(StatusCode, Json<TradeRequest>), AppError> {
    validate_input(&body)?;

    if body.rate_per_tick == 0 {
        return Err(TradeRequestError::ZeroRate.into());
    }

    // Validate good exists and is not transient
    if !state.config.economy.goods.is_empty() {
        if !state.config.economy.goods.iter().any(|g| g.id == body.good_name) {
            return Err(TradeRequestError::UnknownGood(body.good_name.clone()).into());
        }
    }
    if state.config.economy.transient_goods.contains(&body.good_name) {
        return Err(TradeRequestError::TransientGood(body.good_name.clone()).into());
    }

    // Validate mode-specific fields
    match body.mode {
        TradeRequestMode::Total => {
            if body.total_quantity.is_none() {
                return Err(TradeRequestError::TotalQuantityRequired.into());
            }
            if body.price_limit.is_some() {
                return Err(TradeRequestError::TotalNoPriceLimit.into());
            }
        }
        TradeRequestMode::PriceLimit => {
            if body.price_limit.is_none() {
                return Err(TradeRequestError::PriceLimitRequired.into());
            }
            if body.total_quantity.is_some() {
                return Err(TradeRequestError::PriceLimitNoTotalQuantity.into());
            }
        }
    }

    // Validate planet is Connected and player owns the station
    {
        let galaxy = state.galaxy.read().await;
        let mut found = false;
        for system in galaxy.systems.values() {
            for planet in &system.planets {
                if planet.id == body.planet_id {
                    found = true;
                    match &planet.status {
                        PlanetStatus::Connected { station, .. } => {
                            if station.owner_id != auth.0.id {
                                return Err(TradeRequestError::NotStationOwner(
                                    body.planet_id.clone(),
                                )
                                .into());
                            }
                        }
                        _ => {
                            return Err(TradeRequestError::PlanetNotConnected(
                                body.planet_id.clone(),
                            )
                            .into());
                        }
                    }
                }
            }
        }
        if !found {
            return Err(AppError::PlanetNotFound(body.planet_id.clone()));
        }
    }

    let request = TradeRequest {
        id: Uuid::new_v4(),
        owner_id: auth.0.id.clone(),
        planet_id: body.planet_id,
        good_name: body.good_name,
        direction: body.direction,
        mode: body.mode,
        rate_per_tick: body.rate_per_tick,
        total_quantity: body.total_quantity,
        price_limit: body.price_limit,
        cumulative_generated: 0,
        status: TradeRequestStatus::Active,
        created_at: now_ms(),
        completed_at: None,
    };

    let request_id = request.id;

    {
        let mut requests = state.trade_requests.write().await;
        requests.insert(request_id, request.clone());
    }

    spawn_trade_request_loop(
        state.trade_requests.clone(),
        state.galaxy.clone(),
        state.players.clone(),
        state.config.clone(),
        request_id,
    );

    Ok((StatusCode::CREATED, Json(request)))
}

#[utoipa::path(
    get,
    path = "/trade",
    tag = "trade",
    security(("api_key" = [])),
    responses(
        (status = 200, description = "List of trade requests", body = Vec<TradeRequest>),
    ),
)]
#[instrument(skip(state, auth))]
pub async fn list_trade_requests(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
) -> Json<Vec<TradeRequest>> {
    let requests = state.trade_requests.read().await;
    let result: Vec<TradeRequest> = requests
        .values()
        .filter(|r| r.owner_id == auth.0.id)
        .cloned()
        .collect();
    Json(result)
}

#[utoipa::path(
    get,
    path = "/trade/{request_id}",
    tag = "trade",
    security(("api_key" = [])),
    params(
        ("request_id" = Uuid, Path, description = "Trade request ID"),
    ),
    responses(
        (status = 200, description = "Trade request details", body = TradeRequest),
    ),
)]
#[instrument(skip(state, auth))]
pub async fn get_trade_request(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(request_id): Path<Uuid>,
) -> Result<Json<TradeRequest>, AppError> {
    let requests = state.trade_requests.read().await;
    let request = requests
        .get(&request_id)
        .ok_or_else(|| TradeRequestError::RequestNotFound(request_id.to_string()))?;
    if request.owner_id != auth.0.id {
        return Err(TradeRequestError::RequestNotFound(request_id.to_string()).into());
    }
    Ok(Json(request.clone()))
}

#[utoipa::path(
    delete,
    path = "/trade/{request_id}",
    tag = "trade",
    security(("api_key" = [])),
    params(
        ("request_id" = Uuid, Path, description = "Trade request ID"),
    ),
    responses(
        (status = 200, description = "Trade request cancelled", body = TradeRequest),
    ),
)]
#[instrument(skip(state, auth))]
pub async fn cancel_trade_request(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(request_id): Path<Uuid>,
) -> Result<Json<TradeRequest>, AppError> {
    let mut requests = state.trade_requests.write().await;
    let request = requests
        .get_mut(&request_id)
        .ok_or_else(|| TradeRequestError::RequestNotFound(request_id.to_string()))?;
    if request.owner_id != auth.0.id {
        return Err(TradeRequestError::RequestNotFound(request_id.to_string()).into());
    }
    if request.status != TradeRequestStatus::Active {
        return Err(TradeRequestError::RequestNotActive(request_id.to_string()).into());
    }
    request.status = TradeRequestStatus::Cancelled;
    request.completed_at = Some(now_ms());
    Ok(Json(request.clone()))
}
