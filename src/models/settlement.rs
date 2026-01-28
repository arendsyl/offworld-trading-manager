use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Economy {
    // Placeholder for economic simulation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settlement {
    pub name: String,
    pub population: u64,
    pub economy: Economy,
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
