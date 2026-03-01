use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    Limit,
    Market,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Order {
    pub id: Uuid,
    pub player_id: String,
    pub good_name: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub price: Option<u64>,
    pub quantity: u64,
    pub filled_quantity: u64,
    pub status: OrderStatus,
    pub station_planet_id: String,
    pub created_at: u64,
}

impl Order {
    pub fn remaining(&self) -> u64 {
        self.quantity - self.filled_quantity
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TradeEvent {
    pub id: Uuid,
    pub good_name: String,
    pub price: u64,
    pub quantity: u64,
    pub buyer_id: String,
    pub seller_id: String,
    pub buyer_station: String,
    pub seller_station: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct PlaceOrderRequest {
    #[validate(length(min = 1, max = 64))]
    pub good_name: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub price: Option<u64>,
    #[validate(range(min = 1, max = 1_000_000))]
    pub quantity: u64,
    #[validate(length(min = 1, max = 128))]
    pub station_planet_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderBookSummary {
    pub good_name: String,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub last_trade_price: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PriceLevel {
    pub price: u64,
    pub total_quantity: u64,
    pub order_count: u32,
}
