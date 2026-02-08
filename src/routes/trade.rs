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
        .unwrap()
        .as_millis() as u64
}

#[instrument(skip(state, auth))]
async fn create_trade_request(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Json(body): Json<CreateTradeRequestBody>,
) -> Result<(StatusCode, Json<TradeRequest>), AppError> {
    // Validate mode-specific fields
    if body.rate_per_tick == 0 {
        return Err(TradeRequestError::ZeroRate.into());
    }
    match body.mode {
        TradeRequestMode::FixedRate => {
            if body.total_quantity.is_none() {
                return Err(TradeRequestError::TotalQuantityRequired.into());
            }
        }
        TradeRequestMode::Threshold => {
            if body.target_level.is_none() {
                return Err(TradeRequestError::TargetLevelRequired.into());
            }
        }
        TradeRequestMode::Standing => {}
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
        target_level: body.target_level,
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
        state.config.clone(),
        request_id,
    );

    Ok((StatusCode::CREATED, Json(request)))
}

#[instrument(skip(state, auth))]
async fn list_trade_requests(
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

#[instrument(skip(state, auth))]
async fn get_trade_request(
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

#[instrument(skip(state, auth))]
async fn cancel_trade_request(
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
