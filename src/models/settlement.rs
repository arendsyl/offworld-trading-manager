use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::economy::models::EconomyState;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Settlement {
    pub name: String,
    #[serde(default)]
    pub economy: EconomyState,
    #[serde(default)]
    pub founding_goods: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct CreateSettlementRequest {
    #[validate(length(min = 1, max = 128))]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct UpdateSettlementRequest {
    #[validate(length(min = 1, max = 128))]
    pub name: Option<String>,
}
