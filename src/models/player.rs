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
    #[serde(default)]
    pub pulsar_biscuit: String,
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
pub struct CreatePlayerRequest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub credits: Option<i64>,
    pub callback_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSelfView {
    pub id: String,
    pub name: String,
    pub credits: i64,
    pub api_key: String,
    pub callback_url: String,
    pub pulsar_biscuit: String,
}

impl From<&Player> for PlayerSelfView {
    fn from(player: &Player) -> Self {
        Self {
            id: player.id.clone(),
            name: player.name.clone(),
            credits: player.credits,
            api_key: player.api_key.clone(),
            callback_url: player.callback_url.clone(),
            pulsar_biscuit: player.pulsar_biscuit.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePlayerRequest {
    pub callback_url: Option<String>,
    pub name: Option<String>,
}
