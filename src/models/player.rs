use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: String,
    pub name: String,
    pub credits: i64,
    #[serde(default)]
    pub initial_credits: i64,
    pub api_key: String,
    pub callback_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LeaderboardEntry {
    pub player_id: String,
    pub player_name: String,
    pub profit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerPublic {
    pub id: String,
    pub name: String,
    pub credits: i64,
}

impl From<&Player> for PlayerPublic {
    fn from(player: &Player) -> Self {
        Self {
            id: player.id.clone(),
            name: player.name.clone(),
            credits: player.credits,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePlayerRequest {
    pub callback_url: Option<String>,
}
