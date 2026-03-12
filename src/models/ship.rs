use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShipStatus {
    InTransitToOrigin,
    AwaitingOriginDockingAuth,
    Loading,
    AwaitingOriginUndockingAuth,
    InTransit,
    AwaitingDockingAuth,
    Unloading,
    AwaitingUndockingAuth,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Ship {
    pub id: Uuid,
    pub owner_id: String,
    pub origin_planet_id: String,
    pub destination_planet_id: String,
    pub cargo: HashMap<String, u64>,
    pub status: ShipStatus,
    pub trade_id: Option<Uuid>,
    pub trucking_id: Option<Uuid>,
    pub fee: Option<u64>,
    pub created_at: u64,
    pub arrival_at: Option<u64>,
    pub operation_complete_at: Option<u64>,
    #[serde(default)]
    pub estimated_arrival_at: Option<u64>,
    #[serde(default)]
    pub callback_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct CreateTruckingRequest {
    #[validate(length(min = 1, max = 128))]
    pub destination_planet_id: String,
    #[validate(length(min = 1, max = 128))]
    pub origin_planet_id: String,
    pub cargo: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DockRequest {
    pub authorized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UndockRequest {
    pub authorized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShipWebhookPayload {
    OriginDockingRequest {
        ship_id: Uuid,
        origin_planet_id: String,
        destination_planet_id: String,
        cargo: HashMap<String, u64>,
    },
    DockingRequest {
        ship_id: Uuid,
        origin_planet_id: String,
        cargo: HashMap<String, u64>,
    },
    ShipDocked {
        ship_id: Uuid,
        status: String,
    },
    ShipComplete {
        ship_id: Uuid,
    },
}
