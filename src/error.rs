use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;
use tracing::warn;
use utoipa::ToSchema;

use crate::models::SpaceElevatorError;

#[derive(Debug, Clone, Error)]
pub enum MassDriverError {
    #[error("Connection not found: {0}")]
    ConnectionNotFound(String),
    #[error("No channel available on station at planet: {0}")]
    NoChannelAvailable(String),
    #[error("Planets are in different systems")]
    DifferentSystems,
    #[error("Planet is not connected: {0}")]
    PlanetNotConnected(String),
    #[error("Invalid connection state for this action")]
    InvalidConnectionState,
    #[error("Packet too large: {size} > {max} max")]
    PacketTooLarge { size: u64, max: u64 },
    #[error("Insufficient inventory: {good_name} (need {requested}, have {available})")]
    InsufficientInventory {
        good_name: String,
        requested: u64,
        available: u64,
    },
    #[error("Connection is not active")]
    ConnectionNotActive,
    #[error("Cannot create connection to the same station")]
    SameStation,
}

#[derive(Debug, Clone, Error)]
pub enum ShipError {
    #[error("Ship not found: {0}")]
    ShipNotFound(String),
    #[error("Invalid ship state for this action")]
    InvalidShipState,
    #[error("Not the owner of the destination station")]
    NotStationOwner,
    #[error("Insufficient cargo at origin station: {good_name} (need {requested}, have {available})")]
    InsufficientCargo {
        good_name: String,
        requested: u64,
        available: u64,
    },
    #[error("Cannot ship to the same station")]
    SameStation,
}

#[derive(Debug, Clone, Error)]
pub enum MarketError {
    #[error("Insufficient credits: need {needed}, have {available}")]
    InsufficientCredits { needed: i64, available: i64 },
    #[error("Insufficient inventory at station: {good_name} (need {requested}, have {available})")]
    InsufficientInventory {
        good_name: String,
        requested: u64,
        available: u64,
    },
    #[error("Order not found: {0}")]
    OrderNotFound(String),
    #[error("Order cannot be cancelled in current state")]
    OrderNotCancellable,
    #[error("No match available for market order")]
    NoMatchForMarketOrder,
    #[error("Price is required for limit orders")]
    PriceRequired,
    #[error("Station not found for order: {0}")]
    StationNotFoundForOrder(String),
}

#[derive(Debug, Clone, Error)]
pub enum TruckingError {
    #[error("Cannot truck to the same station")]
    SameStation,
    #[error("Not the owner of the origin station")]
    NotOriginStationOwner,
    #[error("Insufficient credits: need {needed}, have {available}")]
    InsufficientCredits { needed: u64, available: i64 },
    #[error("Origin station not found: {0}")]
    OriginStationNotFound(String),
    #[error("Destination station not found: {0}")]
    DestinationStationNotFound(String),
}

#[derive(Debug, Clone, Error)]
pub enum ConstructionError {
    #[error("Insufficient credits: need {needed}, have {available}")]
    InsufficientCredits { needed: u64, available: i64 },
    #[error("Insufficient goods: {good_name} (need {requested}, have {available})")]
    InsufficientGoods {
        good_name: String,
        requested: u64,
        available: u64,
    },
    #[error("Source station not found: {0}")]
    SourceStationNotFound(String),
    #[error("Target planet not found: {0}")]
    TargetPlanetNotFound(String),
    #[error("Target planet is not settled: {0}")]
    TargetNotSettled(String),
    #[error("Target planet already has a station: {0}")]
    TargetAlreadyConnected(String),
    #[error("Target planet is not uninhabited: {0}")]
    TargetNotUninhabited(String),
    #[error("Not the owner of the source station")]
    NotSourceStationOwner,
    #[error("Not the owner of the target station")]
    NotTargetStationOwner,
    #[error("Station has no mass driver")]
    NoMassDriver,
    #[error("Construction project not found: {0}")]
    ProjectNotFound(String),
    #[error("Source and target cannot be the same planet")]
    SamePlanet,
    #[error("Storage full: current {current}, max {max}, incoming {incoming}")]
    StorageFull { current: u64, max: u64, incoming: u64 },
    #[error("No docking bay available at station: {0}")]
    NoDockingBayAvailable(String),
}

#[derive(Debug, Clone, Error)]
pub enum TradeRequestError {
    #[error("Trade request not found: {0}")]
    RequestNotFound(String),
    #[error("Planet is not connected: {0}")]
    PlanetNotConnected(String),
    #[error("Not the owner of the station on planet: {0}")]
    NotStationOwner(String),
    #[error("total_quantity is required for Total mode")]
    TotalQuantityRequired,
    #[error("price_limit is required for PriceLimit mode")]
    PriceLimitRequired,
    #[error("rate_per_tick must be greater than zero")]
    ZeroRate,
    #[error("Trade request is not active: {0}")]
    RequestNotActive(String),
    #[error("Total mode does not accept price_limit")]
    TotalNoPriceLimit,
    #[error("PriceLimit mode does not accept total_quantity")]
    PriceLimitNoTotalQuantity,
    #[error("Unknown good: {0}")]
    UnknownGood(String),
    #[error("Cannot trade transient good: {0}")]
    TransientGood(String),
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("System not found: {0}")]
    SystemNotFound(String),

    #[error("Planet not found: {0}")]
    PlanetNotFound(String),

    #[error("Settlement not found on planet: {0}")]
    SettlementNotFound(String),

    #[error("Station not found on planet: {0}")]
    StationNotFound(String),

    #[error("Planet already exists: {0}")]
    PlanetAlreadyExists(String),

    #[error("Planet is uninhabited, settlement required: {0}")]
    SettlementRequired(String),

    #[error("Planet is not connected (no station/space elevator): {0}")]
    NotConnected(String),

    #[error("{0}")]
    SpaceElevator(#[from] SpaceElevatorError),

    #[error("{0}")]
    MassDriver(#[from] MassDriverError),

    #[error("Player not found: {0}")]
    PlayerNotFound(String),

    #[error("Player already exists: {0}")]
    PlayerAlreadyExists(String),

    #[error("Unauthorized: missing or invalid Bearer token")]
    Unauthorized,

    #[error("Forbidden: you do not have permission")]
    Forbidden,

    #[error("{0}")]
    Ship(#[from] ShipError),

    #[error("{0}")]
    Market(#[from] MarketError),

    #[error("{0}")]
    Trucking(#[from] TruckingError),

    #[error("{0}")]
    Construction(#[from] ConstructionError),

    #[error("{0}")]
    TradeRequest(#[from] TradeRequestError),

    #[error("Station has active ships and cannot be deleted: {0}")]
    StationHasActiveShips(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        warn!(error = %self, "Request failed with error");
        let (status, message) = match &self {
            AppError::SystemNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::PlanetNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::SettlementNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::StationNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::PlanetAlreadyExists(_) => (StatusCode::CONFLICT, self.to_string()),
            AppError::SettlementRequired(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::NotConnected(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::SpaceElevator(e) => {
                let status = match e {
                    SpaceElevatorError::NoCabinAvailable => StatusCode::SERVICE_UNAVAILABLE,
                    SpaceElevatorError::InsufficientStock { .. } => StatusCode::BAD_REQUEST,
                    SpaceElevatorError::ExceedsCapacity { .. } => StatusCode::BAD_REQUEST,
                    SpaceElevatorError::EmptyTransfer => StatusCode::BAD_REQUEST,
                };
                (status, self.to_string())
            }
            AppError::MassDriver(e) => {
                let status = match e {
                    MassDriverError::ConnectionNotFound(_) => StatusCode::NOT_FOUND,
                    MassDriverError::NoChannelAvailable(_) => StatusCode::SERVICE_UNAVAILABLE,
                    MassDriverError::DifferentSystems => StatusCode::BAD_REQUEST,
                    MassDriverError::PlanetNotConnected(_) => StatusCode::BAD_REQUEST,
                    MassDriverError::InvalidConnectionState => StatusCode::CONFLICT,
                    MassDriverError::PacketTooLarge { .. } => StatusCode::BAD_REQUEST,
                    MassDriverError::InsufficientInventory { .. } => StatusCode::BAD_REQUEST,
                    MassDriverError::ConnectionNotActive => StatusCode::CONFLICT,
                    MassDriverError::SameStation => StatusCode::BAD_REQUEST,
                };
                (status, self.to_string())
            }
            AppError::PlayerNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::PlayerAlreadyExists(_) => (StatusCode::CONFLICT, self.to_string()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            AppError::Ship(e) => {
                let status = match e {
                    ShipError::ShipNotFound(_) => StatusCode::NOT_FOUND,
                    ShipError::InvalidShipState => StatusCode::CONFLICT,
                    ShipError::NotStationOwner => StatusCode::FORBIDDEN,
                    ShipError::InsufficientCargo { .. } => StatusCode::BAD_REQUEST,
                    ShipError::SameStation => StatusCode::BAD_REQUEST,
                };
                (status, self.to_string())
            }
            AppError::StationHasActiveShips(_) => (StatusCode::CONFLICT, self.to_string()),
            AppError::Trucking(e) => {
                let status = match e {
                    TruckingError::SameStation => StatusCode::BAD_REQUEST,
                    TruckingError::NotOriginStationOwner => StatusCode::FORBIDDEN,
                    TruckingError::InsufficientCredits { .. } => StatusCode::BAD_REQUEST,
                    TruckingError::OriginStationNotFound(_) => StatusCode::NOT_FOUND,
                    TruckingError::DestinationStationNotFound(_) => StatusCode::NOT_FOUND,
                };
                (status, self.to_string())
            }
            AppError::Market(e) => {
                let status = match e {
                    MarketError::InsufficientCredits { .. } => StatusCode::BAD_REQUEST,
                    MarketError::InsufficientInventory { .. } => StatusCode::BAD_REQUEST,
                    MarketError::OrderNotFound(_) => StatusCode::NOT_FOUND,
                    MarketError::OrderNotCancellable => StatusCode::CONFLICT,
                    MarketError::NoMatchForMarketOrder => StatusCode::BAD_REQUEST,
                    MarketError::PriceRequired => StatusCode::BAD_REQUEST,
                    MarketError::StationNotFoundForOrder(_) => StatusCode::NOT_FOUND,
                };
                (status, self.to_string())
            }
            AppError::TradeRequest(e) => {
                let status = match e {
                    TradeRequestError::RequestNotFound(_) => StatusCode::NOT_FOUND,
                    TradeRequestError::PlanetNotConnected(_) => StatusCode::BAD_REQUEST,
                    TradeRequestError::NotStationOwner(_) => StatusCode::FORBIDDEN,
                    TradeRequestError::TotalQuantityRequired => StatusCode::BAD_REQUEST,
                    TradeRequestError::PriceLimitRequired => StatusCode::BAD_REQUEST,
                    TradeRequestError::ZeroRate => StatusCode::BAD_REQUEST,
                    TradeRequestError::RequestNotActive(_) => StatusCode::CONFLICT,
                    TradeRequestError::TotalNoPriceLimit => StatusCode::BAD_REQUEST,
                    TradeRequestError::PriceLimitNoTotalQuantity => StatusCode::BAD_REQUEST,
                    TradeRequestError::UnknownGood(_) => StatusCode::BAD_REQUEST,
                    TradeRequestError::TransientGood(_) => StatusCode::BAD_REQUEST,
                };
                (status, self.to_string())
            }
            AppError::Construction(e) => {
                let status = match e {
                    ConstructionError::InsufficientCredits { .. } => StatusCode::BAD_REQUEST,
                    ConstructionError::InsufficientGoods { .. } => StatusCode::BAD_REQUEST,
                    ConstructionError::SourceStationNotFound(_) => StatusCode::NOT_FOUND,
                    ConstructionError::TargetPlanetNotFound(_) => StatusCode::NOT_FOUND,
                    ConstructionError::TargetNotSettled(_) => StatusCode::BAD_REQUEST,
                    ConstructionError::TargetAlreadyConnected(_) => StatusCode::CONFLICT,
                    ConstructionError::TargetNotUninhabited(_) => StatusCode::BAD_REQUEST,
                    ConstructionError::NotSourceStationOwner => StatusCode::FORBIDDEN,
                    ConstructionError::NotTargetStationOwner => StatusCode::FORBIDDEN,
                    ConstructionError::NoMassDriver => StatusCode::BAD_REQUEST,
                    ConstructionError::ProjectNotFound(_) => StatusCode::NOT_FOUND,
                    ConstructionError::SamePlanet => StatusCode::BAD_REQUEST,
                    ConstructionError::StorageFull { .. } => StatusCode::BAD_REQUEST,
                    ConstructionError::NoDockingBayAvailable(_) => StatusCode::SERVICE_UNAVAILABLE,
                };
                (status, self.to_string())
            }
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, self.to_string()),
        };

        (status, Json(ErrorResponse { error: message })).into_response()
    }
}
