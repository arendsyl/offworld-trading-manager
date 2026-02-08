use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Economy {
    #[serde(default)]
    pub supply: HashMap<String, u64>,
    #[serde(default)]
    pub demand: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settlement {
    pub name: String,
    pub population: u64,
    pub economy: Economy,
    #[serde(default)]
    pub founding_goods: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSettlementRequest {
    pub name: String,
    pub population: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSettlementRequest {
    pub name: Option<String>,
    pub population: Option<u64>,
}
