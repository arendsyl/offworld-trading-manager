use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Inventory type: maps good name to quantity
pub type Inventory = HashMap<String, u64>;

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct Warehouse {
    pub owner_id: String,
    #[serde(default)]
    pub inventory: Inventory,
}
