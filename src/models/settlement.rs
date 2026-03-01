use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct Economy {
    #[serde(default)]
    pub supply: HashMap<String, u64>,
    #[serde(default)]
    pub demand: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Settlement {
    pub name: String,
    pub population: u64,
    pub economy: Economy,
    #[serde(default)]
    pub founding_goods: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct CreateSettlementRequest {
    #[validate(length(min = 1, max = 128))]
    pub name: String,
    pub population: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct UpdateSettlementRequest {
    #[validate(length(min = 1, max = 128))]
    pub name: Option<String>,
    pub population: Option<u64>,
}
