use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use tracing::instrument;

use crate::auth::AuthenticatedPlayer;
use crate::error::AppError;
use crate::models::{PlayerPublic, UpdatePlayerRequest};
use crate::state::AppState;

pub fn admin_players_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_players))
        .route("/{player_id}", get(get_player))
}

pub fn player_players_router() -> Router<AppState> {
    Router::new()
        .route("/{player_id}", get(get_player).put(update_player))
}

#[instrument(skip(state))]
async fn list_players(State(state): State<AppState>) -> Json<Vec<PlayerPublic>> {
    let players = state.players.read().await;
    let public: Vec<PlayerPublic> = players.values().map(PlayerPublic::from).collect();
    Json(public)
}

#[instrument(skip(state))]
async fn get_player(
    State(state): State<AppState>,
    Path(player_id): Path<String>,
) -> Result<Json<PlayerPublic>, AppError> {
    let players = state.players.read().await;
    let player = players
        .get(&player_id)
        .ok_or_else(|| AppError::PlayerNotFound(player_id))?;
    Ok(Json(PlayerPublic::from(player)))
}

#[instrument(skip(state, auth))]
async fn update_player(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(player_id): Path<String>,
    Json(body): Json<UpdatePlayerRequest>,
) -> Result<(StatusCode, Json<PlayerPublic>), AppError> {
    if auth.0.id != player_id {
        return Err(AppError::Forbidden);
    }

    let mut players = state.players.write().await;
    let player = players
        .get_mut(&player_id)
        .ok_or_else(|| AppError::PlayerNotFound(player_id))?;

    if let Some(callback_url) = body.callback_url {
        player.callback_url = callback_url;
    }

    let public = PlayerPublic::from(&*player);
    Ok((StatusCode::OK, Json(public)))
}
