use axum::{extract::State, routing::get, Json, Router};

use crate::models::LeaderboardEntry;
use crate::state::AppState;

pub fn player_leaderboard_router() -> Router<AppState> {
    Router::new().route("/", get(get_leaderboard))
}

#[utoipa::path(
    get,
    path = "/leaderboard",
    tag = "leaderboard",
    security(("api_key" = [])),
    responses(
        (status = 200, description = "Player leaderboard ranked by profit", body = Vec<LeaderboardEntry>),
    ),
)]
pub async fn get_leaderboard(State(state): State<AppState>) -> Json<Vec<LeaderboardEntry>> {
    let players = state.players.read().await;
    let mut entries: Vec<LeaderboardEntry> = players
        .values()
        .map(|p| LeaderboardEntry {
            player_id: p.id.clone(),
            player_name: p.name.clone(),
            profit: p.credits - p.initial_credits,
        })
        .collect();
    entries.sort_by(|a, b| b.profit.cmp(&a.profit));
    Json(entries)
}
