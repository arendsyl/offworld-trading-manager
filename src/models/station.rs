use serde::{Deserialize, Serialize};

use super::{Inventory, MassDriver};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub name: String,
    pub owner_id: String,
    #[serde(default)]
    pub inventory: Inventory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mass_driver: Option<MassDriver>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStationRequest {
    pub name: String,
    pub owner_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStationRequest {
    pub name: Option<String>,
    pub owner_id: Option<String>,
}
