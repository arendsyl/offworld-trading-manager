use serde::{Deserialize, Serialize};

use super::{Inventory, MassDriver};

fn default_docking_bays() -> u32 {
    2
}

fn default_max_storage() -> u64 {
    u64::MAX
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub name: String,
    pub owner_id: String,
    #[serde(default)]
    pub inventory: Inventory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mass_driver: Option<MassDriver>,
    #[serde(default = "default_docking_bays")]
    pub docking_bays: u32,
    #[serde(default = "default_max_storage")]
    pub max_storage: u64,
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
