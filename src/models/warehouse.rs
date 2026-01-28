use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Inventory type: maps good name to quantity
pub type Inventory = HashMap<String, u64>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Warehouse {
    pub owner_id: String,
    #[serde(default)]
    pub inventory: Inventory,
}
