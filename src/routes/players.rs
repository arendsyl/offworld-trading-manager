use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use tracing::instrument;
use uuid::Uuid;

use crate::auth::AuthenticatedPlayer;
use crate::config::PulsarConfig;
use crate::consumer::spawn_send_consumer;
use crate::error::AppError;
use crate::models::{
    CreatePlayerRequest, OrderSide, OrderStatus, PlanetStatus, Player, PlayerPublic,
    PlayerSelfView, UpdatePlayerRequest,
};
use crate::state::AppState;
use crate::validation::validate_input;

pub fn admin_players_router() -> Router<AppState> {
    Router::new()
        .route("/", get(admin_list_players).post(admin_create_player))
        .route("/{player_id}", get(admin_get_player).delete(admin_delete_player))
}

pub fn player_players_router() -> Router<AppState> {
    Router::new()
        .route("/{player_id}", get(player_get_self).put(player_update_self))
        .route("/{player_id}/regenerate-token", post(player_regenerate_token))
}

fn generate_biscuit(
    root: &biscuit_auth::KeyPair,
    player_id: &str,
    pulsar_config: &PulsarConfig,
) -> Result<String, AppError> {
    use biscuit_auth::macros::biscuit;

    let topic_receive = format!(
        "persistent://{}/{}/mass-driver.receive.{}",
        pulsar_config.tenant, pulsar_config.namespace, player_id
    );
    let topic_send = format!(
        "persistent://{}/{}/mass-driver.send.{}",
        pulsar_config.tenant, pulsar_config.namespace, player_id
    );

    let biscuit = biscuit!(
        r#"
        player({player_id});
        topic({topic_receive});
        topic({topic_send});
        "#
    )
    .build(root)
    .map_err(|e| AppError::Internal(format!("failed to build biscuit token: {e}")))?;

    biscuit
        .to_base64()
        .map_err(|e| AppError::Internal(format!("failed to serialize biscuit: {e}")))
}

// --- Admin routes ---

#[utoipa::path(
    get,
    path = "/admin/players",
    tag = "players",
    responses(
        (status = 200, description = "List of players", body = Vec<PlayerPublic>),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state))]
pub async fn admin_list_players(State(state): State<AppState>) -> Json<Vec<PlayerPublic>> {
    let players = state.players.read().await;
    let public: Vec<PlayerPublic> = players.values().map(PlayerPublic::from).collect();
    Json(public)
}

#[utoipa::path(
    get,
    path = "/admin/players/{player_id}",
    tag = "players",
    params(
        ("player_id" = String, Path, description = "Player ID"),
    ),
    responses(
        (status = 200, description = "Player details", body = PlayerPublic),
        (status = 404, description = "Player not found"),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state))]
pub async fn admin_get_player(
    State(state): State<AppState>,
    Path(player_id): Path<String>,
) -> Result<Json<PlayerPublic>, AppError> {
    let players = state.players.read().await;
    let player = players
        .get(&player_id)
        .ok_or_else(|| AppError::PlayerNotFound(player_id))?;
    Ok(Json(PlayerPublic::from(player)))
}

#[utoipa::path(
    post,
    path = "/admin/players",
    tag = "players",
    request_body = CreatePlayerRequest,
    responses(
        (status = 201, description = "Player created", body = PlayerSelfView),
        (status = 409, description = "Player already exists"),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state))]
pub async fn admin_create_player(
    State(state): State<AppState>,
    Json(body): Json<CreatePlayerRequest>,
) -> Result<(StatusCode, Json<PlayerSelfView>), AppError> {
    validate_input(&body)?;
    let api_key = Uuid::new_v4().to_string();
    let biscuit_token = generate_biscuit(&state.biscuit_root, &body.id, &state.config.pulsar)?;

    let player = Player {
        id: body.id.clone(),
        name: body.name,
        credits: body.credits.unwrap_or(0),
        initial_credits: body.credits.unwrap_or(0),
        api_key,
        callback_url: body.callback_url.unwrap_or_default(),
        pulsar_biscuit: biscuit_token,
    };

    let view = PlayerSelfView::from(&player);

    {
        let mut players = state.players.write().await;
        if players.contains_key(&body.id) {
            return Err(AppError::PlayerAlreadyExists(body.id));
        }
        players.insert(body.id.clone(), player);
    }

    // Spawn Pulsar send consumer if Pulsar is available
    if let Some(ref pulsar) = state.pulsar {
        spawn_send_consumer(
            state.galaxy.clone(),
            pulsar.clone(),
            state.config.clone(),
            body.id,
        );
    }

    Ok((StatusCode::CREATED, Json(view)))
}

#[utoipa::path(
    delete,
    path = "/admin/players/{player_id}",
    tag = "players",
    params(
        ("player_id" = String, Path, description = "Player ID"),
    ),
    responses(
        (status = 204, description = "Player deleted"),
        (status = 404, description = "Player not found"),
    ),
    security(("bearer_auth" = [])),
)]
#[instrument(skip(state))]
pub async fn admin_delete_player(
    State(state): State<AppState>,
    Path(player_id): Path<String>,
) -> Result<StatusCode, AppError> {
    // Verify player exists
    {
        let players = state.players.read().await;
        if !players.contains_key(&player_id) {
            return Err(AppError::PlayerNotFound(player_id));
        }
    }

    // Remove ships owned by the player
    {
        let mut ships = state.ships.write().await;
        ships.retain(|_, ship| ship.owner_id != player_id);
    }

    // Cancel and remove market orders placed by the player, returning reserved credits/goods
    {
        let mut market = state.market.write().await;
        let player_order_ids: Vec<Uuid> = market
            .orders
            .iter()
            .filter(|(_, o)| {
                o.player_id == player_id
                    && matches!(o.status, OrderStatus::Open | OrderStatus::PartiallyFilled)
            })
            .map(|(id, _)| *id)
            .collect();

        let mut credit_refund: i64 = 0;
        let mut goods_refunds: Vec<(String, String, u64)> = Vec::new(); // (planet_id, good, qty)

        for order_id in player_order_ids {
            if let Some(cancelled) = market.cancel_order(order_id) {
                let remaining = cancelled.quantity - cancelled.filled_quantity;
                match cancelled.side {
                    OrderSide::Buy => {
                        if let Some(price) = cancelled.price {
                            credit_refund += price as i64 * remaining as i64;
                        }
                    }
                    OrderSide::Sell => {
                        goods_refunds.push((
                            cancelled.station_planet_id.clone(),
                            cancelled.good_name.clone(),
                            remaining,
                        ));
                    }
                }
            }
        }

        // Apply credit refund
        if credit_refund > 0 {
            let mut players = state.players.write().await;
            if let Some(player) = players.get_mut(&player_id) {
                player.credits += credit_refund;
            }
        }

        // Return goods to stations
        if !goods_refunds.is_empty() {
            let mut galaxy = state.galaxy.write().await;
            for (planet_id, good_name, qty) in goods_refunds {
                for system in galaxy.systems.values_mut() {
                    for planet in &mut system.planets {
                        if planet.id == planet_id {
                            if let PlanetStatus::Connected {
                                ref mut station, ..
                            } = planet.status
                            {
                                let entry =
                                    station.inventory.entry(good_name.clone()).or_insert(0);
                                *entry += qty;
                            }
                        }
                    }
                }
            }
        }
    }

    // Remove construction projects owned by the player
    {
        let mut projects = state.projects.write().await;
        projects.retain(|_, p| p.owner_id != player_id);
    }

    // Clear station ownership for stations owned by the player
    {
        let mut galaxy = state.galaxy.write().await;
        for system in galaxy.systems.values_mut() {
            for planet in &mut system.planets {
                if let PlanetStatus::Connected {
                    ref mut station, ..
                } = planet.status
                {
                    if station.owner_id == player_id {
                        station.owner_id = String::new();
                    }
                }
            }
        }
    }

    // Finally, remove the player
    {
        let mut players = state.players.write().await;
        players.remove(&player_id);
    }

    Ok(StatusCode::NO_CONTENT)
}

// --- Player routes ---

#[utoipa::path(
    get,
    path = "/players/{player_id}",
    tag = "players",
    params(
        ("player_id" = String, Path, description = "Player ID"),
    ),
    responses(
        (status = 200, description = "Player self view", body = PlayerSelfView),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Player not found"),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state, auth))]
pub async fn player_get_self(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(player_id): Path<String>,
) -> Result<Json<PlayerSelfView>, AppError> {
    if auth.0.id != player_id {
        return Err(AppError::Forbidden);
    }
    let players = state.players.read().await;
    let player = players
        .get(&player_id)
        .ok_or_else(|| AppError::PlayerNotFound(player_id))?;
    Ok(Json(PlayerSelfView::from(player)))
}

#[utoipa::path(
    put,
    path = "/players/{player_id}",
    tag = "players",
    params(
        ("player_id" = String, Path, description = "Player ID"),
    ),
    request_body = UpdatePlayerRequest,
    responses(
        (status = 200, description = "Player updated", body = PlayerSelfView),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Player not found"),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state, auth))]
pub async fn player_update_self(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(player_id): Path<String>,
    Json(body): Json<UpdatePlayerRequest>,
) -> Result<(StatusCode, Json<PlayerSelfView>), AppError> {
    validate_input(&body)?;
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
    if let Some(name) = body.name {
        player.name = name;
    }

    let view = PlayerSelfView::from(&*player);
    Ok((StatusCode::OK, Json(view)))
}

#[utoipa::path(
    post,
    path = "/players/{player_id}/regenerate-token",
    tag = "players",
    params(
        ("player_id" = String, Path, description = "Player ID"),
    ),
    responses(
        (status = 200, description = "Token regenerated", body = PlayerSelfView),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Player not found"),
    ),
    security(("api_key" = [])),
)]
#[instrument(skip(state, auth))]
pub async fn player_regenerate_token(
    State(state): State<AppState>,
    auth: AuthenticatedPlayer,
    Path(player_id): Path<String>,
) -> Result<Json<PlayerSelfView>, AppError> {
    if auth.0.id != player_id {
        return Err(AppError::Forbidden);
    }

    let mut players = state.players.write().await;
    let player = players
        .get_mut(&player_id)
        .ok_or_else(|| AppError::PlayerNotFound(player_id))?;

    player.api_key = Uuid::new_v4().to_string();

    let view = PlayerSelfView::from(&*player);
    Ok(Json(view))
}
