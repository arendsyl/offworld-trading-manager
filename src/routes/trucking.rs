use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use tracing::instrument;
use uuid::Uuid;

use crate::auth::AuthenticatedPlayer;
use crate::error::{AppError, TruckingError};
use crate::models::{CreateTruckingRequest, Ship, ShipStatus};
use crate::ship_lifecycle::{calculate_sol_to_planet_time, spawn_transit_to_origin};
use crate::state::AppState;
use crate::validation::validate_input;

pub fn player_trucking_router() -> Router<AppState> {
    Router::new().route("/", post(create_trucking))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_millis() as u64
}

#[utoipa::path(
    post,
    path = "/trucking",
    tag = "trucking",
    security(("api_key" = [])),
    request_body = CreateTruckingRequest,
    responses(
        (status = 201, description = "Trucking request created", body = Ship),
    ),
)]
#[instrument(skip(state, auth))]
pub async fn create_trucking(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Json(body): Json<CreateTruckingRequest>,
) -> Result<(StatusCode, Json<Ship>), AppError> {
    validate_input(&body)?;
    if body.origin_planet_id == body.destination_planet_id {
        return Err(TruckingError::SameStation.into());
    }

    // Look up origin & destination planet info
    let (origin_coords, origin_au, origin_owner) = {
        let galaxy = state.galaxy.read().await;
        let info = galaxy
            .find_planet_info(&body.origin_planet_id)
            .ok_or_else(|| TruckingError::OriginStationNotFound(body.origin_planet_id.clone()))?;
        (info.1, info.2, info.3)
    };

    // Verify player owns origin station
    if origin_owner != auth.0.id {
        return Err(TruckingError::NotOriginStationOwner.into());
    }

    // Verify destination exists
    {
        let galaxy = state.galaxy.read().await;
        galaxy
            .find_planet_info(&body.destination_planet_id)
            .ok_or_else(|| {
                TruckingError::DestinationStationNotFound(body.destination_planet_id.clone())
            })?;
    }

    // Calculate fee
    let total_cargo: u64 = body.cargo.values().sum();
    let fee = state.config.trucking.base_fee
        + (total_cargo as f64 * state.config.trucking.fee_per_unit) as u64;

    // Deduct fee from player credits
    {
        let mut players = state.players.write().await;
        let player = players
            .get_mut(&auth.0.id)
            .ok_or_else(|| AppError::PlayerNotFound(auth.0.id.clone()))?;
        if player.credits < fee as i64 {
            return Err(TruckingError::InsufficientCredits {
                needed: fee,
                available: player.credits,
            }
            .into());
        }
        player.credits -= fee as i64;
    }

    // Calculate Sol → origin travel time
    let transit_secs = calculate_sol_to_planet_time(&origin_coords, origin_au, &state.config.trucking);

    // Get origin owner callback URL (for origin docking webhook)
    let callback_url = {
        let players = state.players.read().await;
        players
            .get(&origin_owner)
            .map(|p| p.callback_url.clone())
            .unwrap_or_default()
    };

    let now = now_ms();
    let ship = Ship {
        id: Uuid::new_v4(),
        owner_id: auth.0.id.clone(),
        origin_planet_id: body.origin_planet_id,
        destination_planet_id: body.destination_planet_id,
        cargo: body.cargo,
        status: ShipStatus::InTransitToOrigin,
        trade_id: None,
        trucking_id: Some(Uuid::new_v4()),
        fee: Some(fee),
        created_at: now,
        arrival_at: None,
        operation_complete_at: None,
        estimated_arrival_at: Some(now + (transit_secs * 1000.0) as u64),
        callback_url: callback_url.clone(),
    };

    let ship_id = ship.id;
    let ship_clone = ship.clone();

    {
        let mut ships = state.ships.write().await;
        ships.insert(ship_id, ship_clone);
    }

    spawn_transit_to_origin(
        state.ships.clone(),
        ship_id,
        transit_secs,
        callback_url,
        state.config.ship.webhook_timeout_secs,
        state.http_client.clone(),
    );

    Ok((StatusCode::CREATED, Json(ship)))
}
