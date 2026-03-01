use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TradeDirection {
    Import,
    Export,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TradeRequestMode {
    FixedRate,
    Standing,
    Threshold,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TradeRequestStatus {
    Active,
    Completed,
    Cancelled,
    AutoCancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TradeRequest {
    pub id: Uuid,
    pub owner_id: String,
    pub planet_id: String,
    pub good_name: String,
    pub direction: TradeDirection,
    pub mode: TradeRequestMode,
    pub rate_per_tick: u64,
    pub total_quantity: Option<u64>,
    pub target_level: Option<u64>,
    pub cumulative_generated: u64,
    pub status: TradeRequestStatus,
    pub created_at: u64,
    pub completed_at: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, ToSchema, Validate)]
pub struct CreateTradeRequestBody {
    #[validate(length(min = 1, max = 64))]
    pub planet_id: String,
    #[validate(length(min = 1, max = 64))]
    pub good_name: String,
    pub direction: TradeDirection,
    pub mode: TradeRequestMode,
    #[validate(range(min = 1, max = 1_000_000))]
    pub rate_per_tick: u64,
    pub total_quantity: Option<u64>,
    pub target_level: Option<u64>,
}
